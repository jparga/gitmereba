## Qué cambia y por qué

<!-- Describe el cambio. Puedes escribir en español o en inglés / English is welcome. -->

## Lista de comprobación

- [ ] `scripts/verificar.sh` termina con código 0 (fmt, clippy, nextest, deny, audit).
- [ ] Los tests se escribieron primero (TDD en `core`) y cubren el cambio.
- [ ] No se filtran secretos en logs, mensajes de error ni argumentos de proceso.
- [ ] No hay `unwrap`/`expect` fuera de tests.
- [ ] Los commits son conventional, p. ej. `feat(core): reintento del clonado inicial`.
- [ ] He actualizado la documentación o el manual si cambia el comportamiento.
