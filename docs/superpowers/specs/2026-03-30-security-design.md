# PRISM Security — Subsystem Design Spec

**Protocol for Remote Interactive Streaming & Multiplexing**

| Field       | Value                          |
|-------------|--------------------------------|
| Version     | 1.0                            |
| Status      | DRAFT                          |
| Date        | 2026-03-30                     |
| Authors     | Ehsan + Claude                 |
| Parent spec | 2026-03-30-prism-architecture-design.md |
| Architecture reqs | R3, R26, R37                |

This document is the complete security design for PRISM across all phases. It covers identity, pairing, authentication, content filters, activation gates, and security lifecycle. The architecture spec defines *what* Security owns; this spec defines *how*.

---

## Table of Contents

1. [Device Identity](#1-device-identity)
2. [Pairing Model](#2-pairing-model)
3. [Pairing Methods](#3-pairing-methods)
4. [Key Rotation](#4-key-rotation)
5. [Authentication Flow](#5-authentication-flow)
6. [Pre-Authentication & Rate Limiting](#6-pre-authentication--rate-limiting)
7. [0-RTT Reconnection Security](#7-0-rtt-reconnection-security)
8. [Browser Authentication (Phase 2)](#8-browser-authentication-phase-2)
9. [Content Filters](#9-content-filters)
10. [Notification Filters](#10-notification-filters)
11. [Camera & Sensor Activation Gates](#11-camera--sensor-activation-gates)
12. [Filesystem Scope](#12-filesystem-scope)
13. [Stolen Device Threat Model](#13-stolen-device-threat-model)
14. [Security Lifecycle & Phase Mapping](#14-security-lifecycle--phase-mapping)
15. [SecurityGate Trait](#15-securitygate-trait)
16. [File Layout](#16-file-layout)
17. [Testing Strategy](#17-testing-strategy)
18. [Optimizations Index](#18-optimizations-index)

---

## 1. Device Identity

Every device in PRISM has a persistent identity that survives key rotation.

```rust
struct DeviceIdentity {
    device_id: Uuid,                // UUIDv7 (time-sortable, unique)
    display_name: String,           // "Ehsan's Desktop"
    platform: Platform,             // Windows, macOS, Linux, Android, iOS, Browser
    current_key: [u8; 32],          // Curve25519 public key
    created_at: u64,                // unix timestamp
}

enum Platform {
    Windows, MacOS, Linux, Android, iOS, Browser,
}
```

The `device_id` is the stable reference. The public key can rotate. When you revoke a device, you target the `device_id`, not a key.

Identity = Curve25519 public key at the protocol level. DeviceIdentity adds a stable UUID and human-readable metadata on top, so key rotation doesn't break the user's mental model of "which device is this."

### 1.1 Hardware Keystore for Private Keys

Private keys should be stored in hardware security modules where available:

- **Android:** KeyStore with `setIsStrongBoxBacked(true)` → Keymaster/StrongBox
- **iOS:** Secure Enclave via `kSecAttrTokenIDSecureEnclave`
- **Windows:** TPM 2.0 via CNG (Cryptography Next Generation)
- **macOS:** Secure Enclave via `SecKeyCreateRandomKey`
- **Linux:** TPM 2.0 via `tss-esapi` crate, fallback to filesystem with LUKS
- **Browser:** Web Crypto API with `extractable: false`, stored in IndexedDB

Key operations (DH, signing) happen inside the HSM. The private key never leaves the hardware security module. Software fallback (`x25519-dalek`, `ed25519-dalek`) is always available.

```rust
trait CryptoBackend: Send + Sync {
    fn dh(&self, private: &[u8; 32], public: &[u8; 32]) -> [u8; 32];
    fn sign(&self, private: &[u8; 32], message: &[u8]) -> [u8; 64];
    fn verify(&self, public: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool;
}

// Implementations selected at startup based on platform + hardware availability
struct SoftwareCrypto;      // x25519-dalek + ed25519-dalek (~50us DH)
struct AndroidHsmCrypto;    // JNI -> KeyStore -> Keymaster (~15us DH)
struct AppleHsmCrypto;      // Security.framework -> Secure Enclave (~10us DH)
struct WindowsTpmCrypto;    // CNG -> TPM 2.0
```

---

## 2. Pairing Model

Flat pairing, no trust zones. Each device maintains its own bilateral pairing list.

```rust
struct PairingEntry {
    device: DeviceIdentity,
    state: PairingState,
    permissions: ChannelPermissions,
    paired_at: u64,
    last_seen: u64,
}

enum PairingState {
    Unpaired,
    PairingInProgress { method: PairingMethod, expires: u64 },
    Paired,
    Blocked,
}

struct ChannelPermissions {
    display: Permission,
    input: Permission,
    clipboard: Permission,
    fileshare: Permission,
    notify: Permission,
    camera: Permission,
    sensor: Permission,
    filesystem_browse: Permission,
}

enum Permission {
    Allow,      // always permitted
    Deny,       // always blocked
    Ask,        // prompt user on first use per session
}
```

`Ask` means the server prompts the user on first use per session. Example: "Friend's laptop wants clipboard access. Allow?" If no response within 30 seconds, auto-deny (conservative — never auto-allow on timeout).

### 2.1 Pairing is Not Transitive

Device A paired with B, and A paired with C, does NOT mean B and C know each other. Pairings are strictly bilateral. This is intentional — you might pair a work desktop with your phone without wanting it to talk to your personal laptop.

For the common case ("all my devices should know each other"), mesh pairing invitations solve the N*(N-1)/2 problem:

```rust
struct MeshInvite {
    from: DeviceIdentity,           // A — the introducer
    introducing: DeviceIdentity,    // B — the device being introduced
    to: Uuid,                       // C's device_id — the recipient
    signature: [u8; 64],            // A's Ed25519 signature (vouching)
    expires: u64,                   // 5-minute window
}
```

C receives: "Ehsan's Desktop wants to introduce Ehsan's Phone. Accept?" If accepted, C initiates direct pairing with B using B's public key from the invite. Both sides still verify via Noise NK. A's introduction just provides the initial key exchange. Phase 2+.

### 2.2 Pairing Store

Pairing list stored as an encrypted local file. Encryption key derived from the device's Curve25519 private key via HKDF, so the pairing list is unreadable without the identity key.

```rust
struct PairingStore {
    path: PathBuf,
    encryption_key: [u8; 32],          // HKDF(device_secret, "pairing-store")

    // In-memory: copy-on-write snapshot for lock-free reads
    current: Arc<PairingSnapshot>,
    writer: Mutex<PairingWriter>,
}

struct PairingSnapshot {
    by_key: HashMap<[u8; 32], Arc<PairingEntry>>,
    by_device_id: HashMap<Uuid, Arc<PairingEntry>>,
    generation: u64,                    // increments on every write
}
```

Reads (every connection) are lock-free: `Arc::clone()` the current snapshot. Writes (rare — pairing/unpairing) take the mutex, rebuild the snapshot, atomically swap the Arc.

Write coalescing: multiple rapid changes (mesh pairing introduces 3 devices) are batched with a 100ms flush delay. Single `fsync()` for N changes.

---

## 3. Pairing Methods

| Phase | Method | Flow |
|-------|--------|------|
| 1 | **Manual key exchange** | Copy-paste hex public key + device_id. Developer-friendly. |
| 1 | **Tailscale auto-discovery** | Devices in same tailnet auto-discover via Tailscale API. Zero-config. |
| 1 | **Short code (SPAKE2)** | Server shows 6-digit code. Client enters it. SPAKE2 derives shared secret, both sides exchange device identities. Works for headless servers. |
| 2 | **QR code** | Server displays QR: `{device_id, public_key, addr, cert_hash, one_time_token}`. Client scans. |
| 4 | **Coordination service** | Optional cloud service stores encrypted device manifests. Keys never leave devices. |

### 3.1 SPAKE2 Short Code Security

6-digit code = 1,000,000 possible values. Brute-force mitigations:

- **Attempt limit:** Max 5 failed attempts per pairing session. After 5, session cancelled.
- **Exponential backoff:** 1s, 2s, 4s, 8s, 16s between attempts.
- **Session timeout:** Code expires after 5 minutes.
- **Active pairing required:** Server must explicitly enter "pairing mode" (user action). No pairing attempts accepted when pairing mode is off.

---

## 4. Key Rotation

Device generates a new keypair and signs the new public key with the old private key. Since Curve25519 is DH (not signing), we derive an Ed25519 signing key from the Curve25519 private key via the standard birational map.

```rust
struct KeyRotation {
    device_id: Uuid,
    new_public_key: [u8; 32],
    old_key_signature: [u8; 64],    // Ed25519: sign(old_private, new_public_key)
    timestamp: u64,
}
```

All paired devices receive the `KeyRotation` message, verify the signature against the old key, and show a confirmation notification: "Ehsan's Phone rotated its key. Was this you? [Accept] [Revoke]". This catches unauthorized rotations from a compromised key.

Phase 4 addition: quorum-based rotation — key rotation requires confirmation from at least one other paired device owned by the same user.

### 4.1 TLS Certificate Rotation

The TLS cert is derived from the Noise static key (see Section 5). When the key rotates, the TLS cert must also rotate. Active connections continue with the old cert (QUIC connections survive). New connections use the new cert. Paired devices receive the new cert hash via Control channel before the switch.

### 4.2 Forward Secrecy Note

Noise NK provides forward secrecy for the initiator's messages but not the responder's. If the server's static key is later compromised, an attacker who recorded past handshakes could learn which clients connected historically. Data confidentiality is not affected — QUIC/TLS provides its own forward secrecy for all application data. Mitigation: periodic key rotation + hardware keystore makes key compromise harder.

---

## 5. Authentication Flow

### 5.1 Noise NK Inside QUIC (Auth Only)

QUIC/TLS 1.3 handles all encryption. Noise NK runs as application-layer messages inside the encrypted QUIC connection, providing only mutual identity authentication. No double encryption.

```
Client                                              Server
  |                                                      |
  |  1. QUIC Initial (TLS 1.3 ClientHello)               |
  |     ALPN: "prism/1"                                  |
  |  -------->                                           |
  |                                                      |
  |  2. QUIC Handshake (TLS 1.3 ServerHello)             |
  |     Client verifies TLS cert matches known Noise key |
  |  <--------                                           |
  |                                                      |
  |  === QUIC established, TLS encrypts all data ===     |
  |                                                      |
  |  3. First app message (stream 0):                    |
  |     NoiseNK_Init {                                   |
  |       noise_handshake: [u8; 48],                     |
  |       client_caps: ClientCapabilities (JSON),        |
  |       device_identity: DeviceIdentity (JSON),        |
  |     }                                                |
  |  -------->                                           |
  |                                                      |
  |  4. Server:                                          |
  |     a. Process Noise NK (Curve25519 DH, ~50us)       |
  |     b. Extract client's static key                   |
  |     c. Look up in pairing store                      |
  |     d. Unknown + no pairing in progress: silent drop  |
  |     e. Known + paired: authenticated                 |
  |                                                      |
  |  5. Server response (stream 0):                      |
  |     NoiseNK_Response {                               |
  |       noise_response: [u8; 48],                      |
  |       server_caps: ServerCapabilities (JSON),        |
  |       channel_assignments: Vec<ChannelAssignment>,   |
  |       first_keyframe: Option<Bytes>,                 |
  |     }                                                |
  |  <--------                                           |
  |                                                      |
  |  === Mutually authenticated. Streaming begins. ===   |
```

### 5.2 TLS Certificate Bound to Noise Key

The server's self-signed TLS certificate is derived from its Noise static key. This eliminates the "skip cert verification" hack:

```
Server generates Curve25519 static key (Noise identity)
  -> Derives Ed25519 signing key (birational map)
  -> Generates self-signed X.509 cert using that Ed25519 key
  -> TLS cert's public key IS the Noise static key (different encoding)

Client:
  -> Knows server's Noise public key (from pairing)
  -> During TLS handshake, extracts server cert's public key
  -> Verifies it matches the known Noise key
  -> If mismatch: MITM detected, abort
```

No custom cert verifier needed. Standard cert verification callback checks against the known key. An active MITM cannot intercept because they can't produce a valid TLS cert matching the server's Noise key.

### 5.3 Timing Side Channel Prevention

Always run the full Noise DH computation before checking the pairing store. This ensures unknown keys and known keys take the same time to process:

```rust
fn authenticate(&self, noise_init: &[u8]) -> Option<SecurityContext> {
    // Always perform DH (~50us) — even for unknown keys
    let noise_result = self.process_noise(noise_init);

    // Then check pairing (HashSet lookup, ~20ns)
    let client_key = noise_result.ok()?.remote_static?;
    let entry = self.pairing_store.current.by_key.get(&client_key)?;

    // Both paths take ~50us regardless of key status
    Some(self.build_security_context(entry))
}
```

---

## 6. Pre-Authentication & Rate Limiting

### 6.1 Connection Rate Limiter

First line of defense before any crypto. Token bucket per source IP:

```rust
struct ConnectionRateLimiter {
    buckets: DashMap<IpAddr, TokenBucket>,
    max_per_second: u32,     // default: 10
    burst: u32,              // default: 20
}
```

Cost of a rejected connection: one hash lookup + one atomic decrement. No crypto, no allocation. On Tailscale (Phase 1-3), less critical. On public relay (Phase 4), essential.

### 6.2 Pre-Authentication Sequence

True zero-allocation pre-auth (validate before QUIC state) would require embedding identity in QUIC Initial tokens — complex and non-standard. Pragmatic approach:

1. Rate limiter checks source IP (O(1), ~5ns)
2. Accept QUIC TLS handshake (allocates ~2KB connection state)
3. Read first app message (Noise NK init)
4. Validate identity (~50us)
5. Unknown device + no pairing in progress: stop reading, drop connection handle. Do NOT send QUIC CONNECTION_CLOSE (that reveals the server exists). Let the connection idle-timeout silently. From the attacker's perspective, indistinguishable from "host doesn't exist" (R3).
6. Blocked device: same as unknown — silent drop, no response.

An attacker needs ~500K fake connections to use 1GB memory. Combined with the rate limiter (10/s per IP), this would take ~14 hours from a single IP.

---

## 7. 0-RTT Reconnection Security

QUIC 0-RTT data is replayable by design. PRISM restricts 0-RTT to idempotent operations:

```rust
fn is_safe_for_0rtt(channel_id: u16, msg_type: u8) -> bool {
    match channel_id {
        CHANNEL_DISPLAY => true,        // frames are idempotent
        CHANNEL_INPUT => true,          // latest-value-wins
        CHANNEL_AUDIO => true,          // audio frames are idempotent
        CHANNEL_CONTROL => {
            msg_type == MSG_HEARTBEAT   // only heartbeats in 0-RTT
        }
        CHANNEL_CLIPBOARD => false,     // could replay paste
        CHANNEL_FILESHARE => false,     // could replay transfer
        CHANNEL_NOTIFY => false,        // could replay actions
        _ => false,
    }
}
```

Non-idempotent messages received during 0-RTT are rejected. Client re-sends them after the full handshake.

---

## 8. Browser Authentication (Phase 2)

WebTransport uses `serverCertificateHashes` instead of custom cert verification. The browser verifies the server's self-signed cert via its SHA-256 hash.

### 8.1 Browser Pairing Flow

QR code / short code carries the cert hash alongside the Noise key:

```json
{
  "device_id": "uuid",
  "public_key": "hex",
  "addr": "100.x.y.z:9876",
  "cert_hash": "sha256_hex_of_der_cert",
  "one_time_token": "random"
}
```

Browser connects:
```javascript
const transport = new WebTransport("https://100.x.y.z:9876", {
    serverCertificateHashes: [{
        algorithm: "sha-256",
        value: hexToBuffer(pairingData.cert_hash)
    }]
});
```

### 8.2 14-Day Certificate Rotation

`serverCertificateHashes` certs must expire within 14 days (browser security policy). Rotation flow:

```
Day 1:  Server generates cert, hash = H1. Browser connects with H1.
Day 12: Server generates new cert, hash = H2.
        Sends CertRenewal{new_hash: H2} via Control channel.
        Browser stores H2 in IndexedDB.
Day 14: Old cert expires. Server switches to new cert.
        Browser reconnects using H2.

Browser offline during renewal:
        Browser tries stored H2 -- works.
        Browser has no H2 stored -- must re-pair (QR/short code).
```

### 8.3 Browser Identity Persistence

Browser generates Curve25519 keypair via Web Crypto API (`extractable: false`). Stored in IndexedDB. Persistent across sessions but clearable by user. Not hardware-backed, but as secure as browser storage allows.

Phase 4 optional upgrade: WebAuthn/Passkey for hardware-backed browser identity.

---

## 9. Content Filters

Content filters protect against accidental exposure of sensitive data. They are **safety nets, not DLP (Data Loss Prevention) systems.** They catch passwords accidentally synced via clipboard, not a determined attacker exfiltrating data.

### 9.1 Filter Architecture

Filters sit in the data plane via the pre-computed `SecurityContext`. Most channels never filter, and the check is zero-cost for those channels.

```
Packet ready to send
  |
  v
SecurityContext.channel_filters[channel_index]
  |
  +-- AllowAll -------> send (display, input, audio: zero cost)
  +-- Blocked --------> drop silently
  +-- NeedsConfirm ---> queue, notify user, 30s timeout, auto-deny
  +-- FilterActive ---> run ContentFilter.filter(data)
                          +-- Allow -------> send
                          +-- Redact(new) -> send modified
                          +-- Block -------> drop
                          +-- Confirm(msg) -> queue + notify
```

```rust
trait ContentFilter: Send + Sync {
    fn filter(&self, data: &[u8]) -> FilterResult;
    fn description(&self) -> &str;
}

enum FilterResult {
    Allow,
    Redact(Bytes),
    Block,
    Confirm(String),
}
```

### 9.2 Clipboard Filters (Phase 3)

Filter chain evaluated in order, first match wins:

**HighEntropyFilter:** Shannon entropy > 4.5 bits/char AND length 8-128 chars → `Confirm("Clipboard contains a high-entropy string that may be a password. Send to [device]?")`

Entropy calculation — single pass, stack-allocated, ~1us:

```rust
fn shannon_entropy(data: &[u8]) -> f64 {
    let mut counts = [0u32; 256];
    for &byte in data {
        counts[byte as usize] += 1;
    }
    let len = data.len() as f64;
    counts.iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / len;
            -p * p.log2()
        })
        .sum()
}
```

**PatternFilter:** Regex-based detection of known sensitive patterns. Uses `RegexSet` for single-pass evaluation of all patterns regardless of count:

Default patterns:
- AWS keys: `AKIA[0-9A-Z]{16}`
- Credit cards: `\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b`
- SSH private keys: `-----BEGIN (RSA|EC|OPENSSH) PRIVATE KEY-----`
- JWT tokens: `eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+`
- GitHub PAT: `ghp_[A-Za-z0-9]{36}`
- Generic API key: `(api[_-]?key|apikey|secret)[=:]\s*['"]?[A-Za-z0-9]{16,}`

**SizeGateFilter:** Clipboard entries >10MB are handed off to FileShare, not synced inline.

**Rich text handling:** Strip HTML/RTF tags, extract text content, run filters on extracted text. For images and base64: don't attempt OCR. Document explicitly: filters catch accidental exposure, not intentional exfiltration.

**User override:** Any filter can be disabled per device pairing.

### 9.3 Confirm Cache

When a user approves a `Confirm` prompt, the approval is cached so reconnection doesn't re-ask:

```rust
struct ConfirmCache {
    approved: HashSet<u64>,     // hash of approved content
    max_entries: usize,         // 100
    ttl_seconds: u64,           // 300 (5 min)
}
```

Preserved in the session tombstone across reconnections within the tombstone window.

### 9.4 Lazy Filter Initialization

Regex patterns compiled only when the first channel that needs them activates:

```rust
struct LazyFilterChain {
    initialized: AtomicBool,
    chain: OnceLock<ClipboardFilterChain>,
    config: FilterConfig,
}
```

Gaming/display-only sessions never compile regex (~1ms saved, ~50KB memory saved).

---

## 10. Notification Filters (Phase 4)

Notifications filtered at the source (phone side) before transmission. Sensitive content never leaves the device.

```rust
struct NotificationFilterConfig {
    app_rules: HashMap<String, AppNotifAction>,
    priority_threshold: NotifPriority,
    category_allowlist: Vec<String>,
    quiet_hours: Option<QuietHours>,
    dnd_sync: bool,
}

enum AppNotifAction {
    AllowAll,           // forward everything
    AllowSilent,        // forward, suppress sound
    TitlesOnly,         // title + app name only, body redacted
    Block,              // never forward
}
```

### 10.1 Banking App Detection

Static list of known banking app package names that default to `Block`. User must explicitly opt in.

Heuristic for unknown apps:

```rust
fn is_likely_sensitive_app(notif: &Notification) -> bool {
    if KNOWN_BANKING_APPS.contains(&notif.app_id) { return true; }
    if OTP_REGEX.is_match(&notif.body) { return true; }
    if notif.category == Some("finance") { return true; }
    false
}
```

OTP regex: `\b\d{4,8}\b` combined with keywords "code", "OTP", "verify", "authentication", "2FA", "PIN".

### 10.2 Filter Short-Circuit

```rust
fn should_forward(notif: &Notification, config: &NotificationFilterConfig) -> NotifAction {
    // Fast: DND active -> block
    if config.dnd_sync && notif.source_dnd_active { return NotifAction::Block; }
    // Fast: below priority
    if notif.priority < config.priority_threshold { return NotifAction::Block; }
    // Fast: explicit per-app rule
    if let Some(rule) = config.app_rules.get(&notif.app_id) { return rule.clone(); }
    // Slow: heuristic
    if is_likely_sensitive_app(notif) { return NotifAction::TitlesOnly; }
    // Default
    NotifAction::AllowAll
}
```

Common case: 2 comparisons + 1 HashMap lookup = ~30ns.

---

## 11. Camera & Sensor Activation Gates (Phase 4)

Not content filters — activation gates that require per-session consent.

```rust
struct ActivationGate {
    requires_per_session_consent: bool,     // true for camera, GPS
    requires_persistent_indicator: bool,    // true for camera
    auto_timeout_minutes: Option<u32>,      // camera: 60 min
}
```

**Camera flow:**
1. Desktop requests camera channel
2. Server sends CameraRequest to phone
3. Phone shows system dialog: "[Desktop] wants to use your camera. Allow?"
4. User taps Allow → camera stream starts
5. Phone shows persistent notification: "Camera shared with [Desktop]. Tap to stop."
6. After 60 minutes (configurable), camera auto-stops

Camera NEVER auto-starts. Each session requires fresh consent.

**GPS:** Same flow, plus: location data never cached on receiver, persistent indicator on both devices, stops immediately on disconnect.

---

## 12. Filesystem Scope (Phase 3)

```rust
struct FilesystemScope {
    allowed_paths: Vec<PathBuf>,
    requires_auth: FilesystemAuth,
}

enum FilesystemAuth {
    None,                   // browse immediately
    PinCode(String),        // 4-6 digit PIN
    Biometric,              // fingerprint/face (Phase 4)
}
```

Default scopes:
- Phone → Desktop: `~/Downloads`, `~/Documents`, `~/Desktop`
- Desktop → Phone: `/sdcard/Download`, `/sdcard/DCIM`, `/sdcard/Pictures`

Full filesystem access requires explicit opt-in per pairing + PIN/biometric.

---

## 13. Stolen Device Threat Model

Defense in depth:

**Layer 1: OS lock screen.** PRISM enters "frozen" state on device lock — heartbeat continues but no data flow, no new channel activations. Listens for OS lock/unlock events.

**Layer 2: PRISM session lock (optional).** Separate PIN/biometric before any connection, even if device is unlocked.

**Layer 3: Remote revocation.**
- Phase 1-3: Manually remove device from each paired device's list.
- Phase 4: Revoke via coordination service, broadcasts to all paired devices.

**Layer 4: Hardware keystore.** Private key in HSM (Android Keymaster, iOS Secure Enclave, Windows TPM, macOS Secure Enclave). Key extraction requires breaking the hardware.

---

## 14. Security Lifecycle & Phase Mapping

### 14.1 State Machine

```
                    [Discovery]
                        |
                        v
    +-----------> [Unpaired] <-----------+
    |                   |                 |
    |          pairing initiated          |
    |                   v                 |
    |           [PairingInProgress]       |
    |            /              \         |
    |      success              failure   |
    |          /                    |      |
    |         v                    |      |
    |     [Paired] ----------------+      |
    |      /    \                         |
    |   active   idle                     |
    |    /         \                      |
    |  streaming   heartbeat   user revokes
    |    \         /                |
    |     \       /                v
    |      [Paired]          [Blocked]
    |         |                   |
    |    key rotation        user unblocks
    |    (stays Paired)           |
    |                             |
    +-----------------------------+
```

### 14.2 Phase Mapping

| Component | Phase 1 | Phase 2 | Phase 3 | Phase 4 |
|-----------|---------|---------|---------|---------|
| Identity | DeviceIdentity (UUID + key + name) | + Browser identity (Web Crypto) | No change | + Coordination service |
| Pairing | Manual + Tailscale + SPAKE2 | + QR code + Mesh invite | No change | + Coordination discovery |
| Auth | Noise NK inside QUIC, TLS cert bound | + Browser WebTransport path | No change | No change |
| Pre-auth | Accept QUIC, validate, close fast | No change | No change | + Rate limiter (public relay) |
| Key Rotation | Signed rotation + user confirm | No change | No change | + Quorum rotation |
| SecurityGate | Full trait, AllowAll for all channels | No change | + Clipboard + Notify filters | + Camera/Sensor gates |
| Clipboard Filters | Trait defined, no-op | No change | HighEntropy + Pattern + SizeGate | No change |
| Notification Filters | N/A | N/A | N/A | Per-app + OTP heuristic + quiet hours |
| Camera/Sensor Gates | N/A | N/A | N/A | Per-session consent + indicators |
| Filesystem Scope | N/A | N/A | Default scopes + PIN | + Biometric |
| 0-RTT Policy | Idempotent-only | No change | No change | No change |
| Audit Log | Basic (connect/disconnect) | No change | + Filter events | + Permission events |
| Confirm Cache | N/A (no filters) | N/A | Active (clipboard) | Active (clip + notify + camera) |

---

## 15. SecurityGate Trait

The contract that Transport and Session Manager code against from Phase 1:

```rust
trait SecurityGate: Send + Sync {
    /// Authenticate a newly connected client.
    /// Called after QUIC TLS, reads Noise NK init from first stream message.
    /// Returns None = close connection.
    async fn authenticate(
        &self,
        noise_init: &[u8],
        device_identity: &DeviceIdentity,
    ) -> Option<SecurityContext>;

    /// Begin a new pairing flow.
    async fn start_pairing(
        &self,
        method: PairingMethod,
    ) -> Result<PairingHandle, SecurityError>;

    /// Process a pairing message from a device in PairingInProgress state.
    async fn process_pairing(
        &self,
        handle: &PairingHandle,
        message: &[u8],
    ) -> Result<PairingResult, SecurityError>;

    /// Rotate this device's key. Broadcasts to all paired devices.
    async fn rotate_key(&self) -> Result<KeyRotation, SecurityError>;

    /// Process a key rotation from a paired device.
    async fn receive_key_rotation(
        &self,
        rotation: &KeyRotation,
    ) -> Result<KeyRotationResult, SecurityError>;

    /// Get pre-computed security context for a connected client.
    fn security_context(&self, device_id: &Uuid) -> Option<Arc<SecurityContext>>;

    /// Record a security audit event.
    fn audit(&self, event: AuditEvent);
}

/// Pre-computed per-connection security decisions.
struct SecurityContext {
    device: Arc<PairingEntry>,
    channel_filters: [ChannelFilterState; 16],
    is_0rtt_safe: [bool; 16],
    confirm_cache: Mutex<ConfirmCache>,
}

enum ChannelFilterState {
    AllowAll,
    FilterActive(Arc<dyn ContentFilter>),
    Blocked,
    NeedsConfirmation,
}

enum PairingResult {
    Paired(PairingEntry),
    NeedsMoreMessages(Vec<u8>),
    Failed(String),
}

enum KeyRotationResult {
    Accepted,
    NeedsConfirmation(String),
    Rejected(String),
}
```

---

## 16. File Layout

```
crates/prism-security/src/
    lib.rs                  # re-exports
    identity.rs             # DeviceIdentity, Platform, UUID generation
    crypto.rs               # CryptoBackend trait, HKDF, Curve25519<->Ed25519, entropy
    pairing/
        mod.rs              # PairingStore, PairingSnapshot, PairingEntry
        methods/
            mod.rs          # PairingMethod enum, PairingHandle
            manual.rs       # Manual hex key exchange
            tailscale.rs    # Tailscale auto-discovery
            spake2.rs       # Short code pairing
            qr.rs           # QR code pairing (Phase 2)
    handshake.rs            # Noise NK inside QUIC, TLS cert binding
    gate.rs                 # SecurityGate trait + default implementation
    context.rs              # SecurityContext, ChannelFilterState, CachedPermissions
    filters/
        mod.rs              # ContentFilter trait, FilterResult
        clipboard.rs        # HighEntropy, Pattern, SizeGate
        notification.rs     # Per-app rules, OTP heuristic (Phase 4)
    activation.rs           # Camera/Sensor consent gates (Phase 4)
    scope.rs                # Filesystem scope + auth gates (Phase 3)
    audit.rs                # AuditEvent, ring buffer audit log
    key_rotation.rs         # KeyRotation, Ed25519 signing, batch verification
    rate_limit.rs           # Token bucket connection rate limiter
```

---

## 17. Testing Strategy

| Category | What | How |
|----------|------|-----|
| Unit: Identity | UUID generation, key derivation, serialization | Pure functions, no I/O |
| Unit: Filters | Each filter vs known inputs (passwords, CC, OTP, benign text) | Table-driven tests |
| Unit: Entropy | Shannon entropy vs known strings | "aaaa" = low, random = high |
| Unit: Pairing Store | Save/load/search, snapshot consistency | Tempdir, concurrent R/W |
| Integration: Handshake | Full Noise NK inside QUIC between two identities | Two quinn endpoints, localhost |
| Integration: Pre-auth | Unknown device closed, no crash, no state leak | Random key, verify server health |
| Integration: SPAKE2 | Correct code succeeds, wrong code fails after 5 attempts | Two processes, simulated input |
| Integration: Key Rotation | Rotate on A, verify B accepts with confirmation | Two paired devices |
| Integration: 0-RTT | Reconnect, verify non-idempotent messages rejected | Disconnect + reconnect |
| Integration: TLS Binding | MITM with wrong cert detected and aborted | Three endpoints (client, MITM, server) |
| Fuzz: Noise parser | Malformed init messages don't crash | cargo-fuzz, random bytes |
| Fuzz: Filters | Arbitrary binary, huge strings, empty strings | cargo-fuzz on filter chain |
| Perf: Allowlist | 10,000 keys, verify O(1) lookup | criterion benchmark |
| Perf: Filter chain | 10 regex patterns on 10KB, verify <100us | criterion benchmark |
| Perf: Auth path | End-to-end authentication, verify <1ms | criterion benchmark |

---

## 18. Optimizations Index

| ID | Optimization | Impact |
|----|-------------|--------|
| O1 | Auth hot path ~55us (DH dominates), no async spawn | Already fast, don't add slow operations |
| O2 | Copy-on-write pairing snapshot, lock-free reads | Zero contention on read path |
| O3 | Token bucket rate limiter per source IP | First defense, ~5ns per check |
| O4 | Ed25519 batch verification for key rotations | 2x faster for multiple rotations |
| O5 | SecurityContext: fixed array, pre-computed at connect time | ~2ns per-frame security check |
| O6 | RegexSet single-pass evaluation for all patterns | O(n) regardless of pattern count |
| O7 | Shannon entropy: stack-allocated, single pass | ~1us, no allocation |
| O8 | Notification filter short-circuit chain | ~30ns common case |
| O9 | Lazy filter initialization via OnceLock | No cost for non-filtering sessions |
| O10 | Hardware-accelerated crypto (HSM) on mobile | 3-5x faster DH |
| O11 | Pairing store write coalescing (100ms batch) | Single fsync for N changes |
| O12 | Audit log as mmap ring buffer, fixed 128B entries | Zero-alloc writes |

---

*PRISM Security Design v1.0 — CC0 Public Domain*
