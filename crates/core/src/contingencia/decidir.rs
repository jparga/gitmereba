//! Decisión, pura y sin E/S, de qué hacer con una rama al reconciliar; y construcción de
//! los argumentos de `git push`, que nunca llevan ninguna forma de fuerza.

/// Qué hacer con una rama al reconciliar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// El remoto coincide con el punto de partida o es antepasado de la rama local: se
    /// envía sin `--force`.
    AvanceRapido,
    /// La rama local es antepasado del remoto: no hay nada que enviar.
    SinCambios,
    /// Ninguna de las dos historias contiene a la otra: no se envía nada, se informa.
    Divergente,
    /// La rama no existe todavía en el remoto: se crea.
    RamaNueva,
}

/// Entrada de [`decidir`]: todo lo que hace falta saber de una rama, ya calculado (esta
/// función no ejecuta ningún comando ni toca disco ni red).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntradaDecision<'a> {
    /// SHA de la rama en el remoto, o `None` si no existe todavía.
    pub remoto: Option<&'a str>,
    /// SHA de la rama en el punto de partida, o `None` si la rama es nueva desde
    /// entonces.
    pub partida: Option<&'a str>,
    pub remoto_es_ancestro_de_local: bool,
    pub local_es_ancestro_de_remoto: bool,
}

/// Decide qué hacer con una rama. Función pura: mismo resultado siempre para la misma
/// entrada.
pub fn decidir(entrada: &EntradaDecision<'_>) -> Decision {
    let Some(remoto) = entrada.remoto else {
        return Decision::RamaNueva;
    };
    let coincide_con_partida = entrada.partida == Some(remoto);
    if coincide_con_partida || entrada.remoto_es_ancestro_de_local {
        Decision::AvanceRapido
    } else if entrada.local_es_ancestro_de_remoto {
        Decision::SinCambios
    } else {
        Decision::Divergente
    }
}

/// Refspec de avance rápido para una rama: sin `+` inicial, nunca fuerza.
pub fn refspec_rama(rama: &str) -> String {
    format!("refs/heads/{rama}:refs/heads/{rama}")
}

/// Refspec para un tag nuevo: sin `+` inicial, nunca fuerza.
pub fn refspec_tag(tag: &str) -> String {
    format!("refs/tags/{tag}:refs/tags/{tag}")
}

/// Argumentos completos de `git push` para una rama o tag, listos para `git::ejecutar`:
/// nunca contienen ninguna forma de fuerza ni de borrado.
pub fn argumentos_push(remoto: &str, refspec: &str) -> Vec<String> {
    vec!["push".to_string(), remoto.to_string(), refspec.to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entrada<'a>(
        remoto: Option<&'a str>,
        partida: Option<&'a str>,
        remoto_es_ancestro_de_local: bool,
        local_es_ancestro_de_remoto: bool,
    ) -> EntradaDecision<'a> {
        EntradaDecision {
            remoto,
            partida,
            remoto_es_ancestro_de_local,
            local_es_ancestro_de_remoto,
        }
    }

    #[test]
    fn tabla_exhaustiva_de_decisiones() {
        // Sin remoto: siempre rama nueva, pase lo que pase en el resto de campos.
        assert_eq!(
            decidir(&entrada(None, None, false, false)),
            Decision::RamaNueva
        );
        assert_eq!(
            decidir(&entrada(None, Some("p"), true, true)),
            Decision::RamaNueva
        );

        // Remoto coincide con el punto de partida: avance rápido, aunque los flags de
        // antepasado (que en la práctica no deberían darse así) digan lo contrario.
        assert_eq!(
            decidir(&entrada(Some("p"), Some("p"), false, false)),
            Decision::AvanceRapido
        );

        // Remoto antepasado de local (y no coincide con la partida): avance rápido.
        assert_eq!(
            decidir(&entrada(Some("r"), Some("p"), true, false)),
            Decision::AvanceRapido
        );

        // Local antepasado de remoto, remoto no antepasado de local: sin cambios.
        assert_eq!(
            decidir(&entrada(Some("r"), Some("p"), false, true)),
            Decision::SinCambios
        );

        // Ninguno antepasado del otro: divergente.
        assert_eq!(
            decidir(&entrada(Some("r"), Some("p"), false, false)),
            Decision::Divergente
        );

        // Caso degenerado (mismo commit, ambos antepasados mutuos): avance rápido, que
        // es un no-op seguro para git.
        assert_eq!(
            decidir(&entrada(Some("r"), Some("p"), true, true)),
            Decision::AvanceRapido
        );

        // Sin punto de partida conocido para la rama (recién descubierta), pero el
        // remoto ya existe y es antepasado: sigue siendo avance rápido.
        assert_eq!(
            decidir(&entrada(Some("r"), None, true, false)),
            Decision::AvanceRapido
        );
        assert_eq!(
            decidir(&entrada(Some("r"), None, false, false)),
            Decision::Divergente
        );
    }

    #[test]
    fn refspecs_nunca_llevan_prefijo_de_fuerza() {
        for rama in ["main", "feature/x-1", "release-2.0"] {
            let refspec = refspec_rama(rama);
            assert!(!refspec.starts_with('+'), "{refspec}");
            assert!(!refspec.starts_with(':'), "{refspec}");
            assert_eq!(refspec, format!("refs/heads/{rama}:refs/heads/{rama}"));
        }
        for tag in ["v1.0", "release-2024.09"] {
            let refspec = refspec_tag(tag);
            assert!(!refspec.starts_with('+'), "{refspec}");
            assert_eq!(refspec, format!("refs/tags/{tag}:refs/tags/{tag}"));
        }
    }

    #[test]
    fn los_argumentos_de_push_nunca_llevan_ninguna_forma_de_fuerza() {
        let prohibidos = [
            "--force",
            "-f",
            "--force-with-lease",
            "--mirror",
            "--delete",
            "--prune",
        ];
        let ramas = ["main", "feature/x-1", "release-2.0", "a/b/c"];
        let tags = ["v1.0", "release-2024.09"];

        let mut refspecs: Vec<String> = ramas.iter().map(|r| refspec_rama(r)).collect();
        refspecs.extend(tags.iter().map(|t| refspec_tag(t)));

        for refspec in &refspecs {
            let args = argumentos_push("https://github.com/acme/demo.git", refspec);
            assert_eq!(args.len(), 3);
            assert_eq!(args[0], "push");
            for arg in &args {
                assert!(!arg.starts_with('+'), "{arg}");
                assert!(!arg.starts_with(':'), "{arg}");
                for prohibido in prohibidos {
                    assert!(!arg.contains(prohibido), "«{arg}» contiene «{prohibido}»");
                }
            }
        }
    }
}
