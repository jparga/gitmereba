# Cómo contribuir · Contributing

**Español** · [English](#english)

Gracias por tu interés. gitmereba es un proyecto pequeño, mantenido por una sola persona,
cuya prioridad es la seguridad. Por eso las reglas son estrictas, pero pocas.

## Antes de empezar

- Para errores y propuestas, abre un issue con la plantilla correspondiente. Para cambios
  grandes, abre primero un issue y acordad el enfoque.
- **Las vulnerabilidades no van a un issue**: sigue [SECURITY.md](SECURITY.md).
- Se aceptan issues y PR en español o en inglés.

## Entorno

Necesitas Linux, `git` y las dependencias de compilación de Tauri 2 (WebKitGTK 4.1, GTK 3,
librsvg, libsoup 3; la lista exacta está en `.github/workflows/ci.yml`). La versión de Rust
la fija `rust-toolchain.toml`. Además, `cargo-nextest`, `cargo-deny` y `cargo-audit`.

```bash
scripts/verificar.sh                          # fmt, clippy -D warnings, nextest, deny, audit
cargo nextest run -p gitmereba-core <filtro>  # un módulo
```

Los tests que levantan un Gitea real se saltan solos salvo que `GITMEREBA_TEST_GITEA`
apunte a un binario de Gitea. Ningún test toca la red ni el llavero real.

## Reglas del código

- **Un cambio no está hecho hasta que `scripts/verificar.sh` sale con 0.**
- TDD en `crates/core`: primero el test que falla y después el código.
- Toda la lógica va en `crates/core`. `app/` solo contiene CLI y ventana, y `ui/` es
  HTML/CSS/JS estático, sin Node, sin bundler y sin CDN.
- `unsafe` está prohibido. Nada de `unwrap`/`expect` fuera de los tests.
- Los secretos viven en el tipo `Secreto` y en el llavero: nunca en ficheros, logs,
  mensajes de error, argumentos de proceso ni URLs.
- `git` y `gitea` se invocan sin shell, con los argumentos como lista y los nombres validados.
- Las dependencias nuevas deben ser las mínimas imprescindibles, con
  `default-features = false` cuando se pueda y TLS con rustls. Además deben pasar
  `cargo deny check`.
- El código, los comentarios, los mensajes y los commits van en español. Los
  identificadores también, salvo los términos de git/API (`mirror`, `token`, `push`).
- En `ui/`, los colores se definen solo con tokens de `ui/css/mereba.css`.
- Commits [conventional](https://www.conventionalcommits.org/es/):
  `feat(core): reintento del clonado inicial`.
- Si cambia el comportamiento visible, actualiza `docs/manual.md` y la ayuda de la vista.

## Licencia de las contribuciones

Al enviar una contribución aceptas que se publique bajo la misma licencia del proyecto,
[GPL-3.0-or-later](LICENSE) (*inbound = outbound*). Firma tus commits con `git commit -s`
([Developer Certificate of Origin](https://developercertificate.org/)).

---

## English

Thanks for your interest! gitmereba is a small, single-maintainer, security-first project.
Issues and PRs in English are welcome.

- Bugs and feature requests: use the issue templates. Discuss large changes in an issue
  first. **Security issues: follow [SECURITY.md](SECURITY.md), never a public issue.**
- Setup: Linux, `git`, Tauri 2 build dependencies (see `.github/workflows/ci.yml`), the
  Rust toolchain pinned in `rust-toolchain.toml`, plus `cargo-nextest`, `cargo-deny` and
  `cargo-audit`. **A change is done only when `scripts/verificar.sh` exits 0.**
- Rules: test-first in `crates/core`; all logic in `core`; no `unsafe`; no
  `unwrap`/`expect` outside tests; secrets only in the `Secreto` type and the keyring,
  never in files, logs, errors, process arguments or URLs; `git`/`gitea` run without a
  shell; minimal dependencies that pass `cargo deny check`; conventional commits.
- Code, comments and messages are in Spanish by project convention. If that is a barrier,
  write in English and the maintainer will adapt it.
- Contributions are licensed under [GPL-3.0-or-later](LICENSE) (inbound = outbound).
  Please sign off your commits (`git commit -s`, DCO).
