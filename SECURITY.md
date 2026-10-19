# Security

Found a vulnerability? Please report it privately through GitHub: **Security → Report a vulnerability** on <https://github.com/volodya-rua/DualAux>. Please don't open a public issue for security problems.

DualAux has no network code, collects no data (see [PRIVACY.md](PRIVACY.md)) and runs no servers, so the realistic surface is local: the helper commands it runs (`pactl` on Linux, a compiled Swift helper on macOS) and the Android permissions it asks for.
