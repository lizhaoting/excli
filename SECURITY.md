# excli Security Policy

## Reporting Security Vulnerabilities

**DO NOT** file public issues for security vulnerabilities.

Instead, please email security details to: **security@excli.io**

Include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Your name and affiliation (optional)

We will:
1. Acknowledge receipt within 48 hours
2. Investigate and assess severity
3. Develop and test a fix
4. Release a patched version
5. Credit you in the release notes (if desired)

## Supported Versions

| Version | Status | Support Until |
|---------|--------|----------------|
| 0.1.x | Current | 2026-05-27 |
| 0.0.x | EOL | 2025-12-31 |

## Security Considerations

### Safe by Design

- No macro execution (prevents VBA-based attacks)
- No arbitrary code execution
- Input validation on all formats
- Safe handling of malformed files

### Dependencies

We regularly audit dependencies with:

```bash
cargo audit
cargo outdated
```

### Known Limitations

- **Macro preservation**: `.xlsm` files retain macro metadata but don't execute
- **External links**: Links to external files are not resolved
- **OLE/VBA**: OLE streams are not fully parsed

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for security-related changes.

---

**Thank you for helping keep excli secure!**
