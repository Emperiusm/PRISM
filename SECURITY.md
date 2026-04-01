# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Instead, please report vulnerabilities privately:

1. **Email:** Send details to info@slabsofficial.com
2. **GitHub:** Use [Security Advisories](../../security/advisories/new) to report privately

### What to Include

- Description of the vulnerability
- Steps to reproduce
- Affected components (e.g., `prism-security`, `prism-transport`)
- Impact assessment (if known)
- Suggested fix (if any)

### Response Timeline

- **Acknowledgment:** Within 48 hours
- **Initial assessment:** Within 7 days
- **Fix or mitigation:** Dependent on severity
  - Critical: Patch within 72 hours of confirmation
  - High: Patch within 14 days
  - Medium/Low: Next scheduled release

### Disclosure Policy

- We follow coordinated disclosure. Please allow up to 90 days before public disclosure.
- Credit will be given to reporters in the release notes (unless anonymity is requested).

## Security Architecture

PRISM uses defense-in-depth:

- **Transport encryption:** QUIC with TLS 1.3 + Noise IK (double encryption)
- **Key exchange:** X25519 ephemeral + Ed25519 static identity keys
- **Payload encryption:** AES-256-GCM with HKDF-derived per-session keys
- **Key rotation:** Automatic rekeying at configurable intervals
- **Authentication:** SPAKE2 pairing + HMAC-based auth tokens
- **Input validation:** Rate limiting, packet size bounds, channel filtering
