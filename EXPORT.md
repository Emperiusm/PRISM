# Export Control Notice

This software contains cryptographic functionality and may be subject to export control regulations.

## Cryptographic Components

PRISM includes the following cryptographic capabilities:

| Component | Algorithm | Purpose |
|-----------|-----------|---------|
| Key exchange | X25519 (Curve25519) | Ephemeral Diffie-Hellman |
| Signatures | Ed25519 | Identity authentication |
| Symmetric encryption | AES-256-GCM | Payload encryption |
| Key derivation | HKDF-SHA256 | Per-session key derivation |
| Noise protocol | Noise IK | Authenticated key exchange |
| TLS | TLS 1.3 (via rustls) | QUIC transport encryption |
| HMAC | HMAC-SHA256 | Auth token generation |
| PAKE | SPAKE2 | Password-authenticated pairing |

## Classification

This software is publicly available open-source software and is believed to qualify for the publicly available exemption under:

- **US EAR:** ECCN 5D002, License Exception TSU (Technology and Software Unrestricted) per 15 CFR 740.13(e) for publicly available encryption source code
- **EU Dual-Use Regulation:** Category 5 Part 2, generally exempt for public domain/open-source software

## Notification

In accordance with EAR 740.13(e), notification of this publicly available encryption source code has been / should be sent to:

- US Bureau of Industry and Security (BIS): crypt@bis.doc.gov
- ENC Encryption Request Coordinator: enc@nsa.gov

## Disclaimer

This notice is provided for informational purposes only and does not constitute legal advice. Users are responsible for ensuring compliance with applicable export control laws in their jurisdiction.
