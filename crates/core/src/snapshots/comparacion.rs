//! Comparación de listas de refs `(nombre, sha)`, sin E/S.

/// Se queda solo con las refs de ramas y tags (`refs/heads/*`, `refs/tags/*`); un bare
/// puede tener otras (`HEAD`, notas...) que no interesan a un snapshot.
pub(super) fn filtrar_ramas_y_tags(refs: Vec<(String, String)>) -> Vec<(String, String)> {
    refs.into_iter()
        .filter(|(nombre, _)| nombre.starts_with("refs/heads/") || nombre.starts_with("refs/tags/"))
        .collect()
}

/// Si `a` y `b` contienen exactamente los mismos pares `(nombre, sha)`, sin importar el
/// orden.
pub(super) fn mismas_refs(a: &[(String, String)], b: &[(String, String)]) -> bool {
    let mut a = a.to_vec();
    let mut b = b.to_vec();
    a.sort();
    b.sort();
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtra_ramas_y_tags_e_ignora_lo_demas() {
        let refs = vec![
            ("refs/heads/main".to_string(), "a".to_string()),
            ("refs/tags/v1".to_string(), "b".to_string()),
            ("refs/pull/1/head".to_string(), "c".to_string()),
        ];
        let filtradas = filtrar_ramas_y_tags(refs);
        assert_eq!(
            filtradas,
            vec![
                ("refs/heads/main".to_string(), "a".to_string()),
                ("refs/tags/v1".to_string(), "b".to_string()),
            ]
        );
    }

    #[test]
    fn mismas_refs_ignora_el_orden() {
        let a = vec![
            ("refs/heads/main".to_string(), "a".to_string()),
            ("refs/tags/v1".to_string(), "b".to_string()),
        ];
        let b = vec![
            ("refs/tags/v1".to_string(), "b".to_string()),
            ("refs/heads/main".to_string(), "a".to_string()),
        ];
        assert!(mismas_refs(&a, &b));
    }

    #[test]
    fn mismas_refs_detecta_diferencias() {
        let a = vec![("refs/heads/main".to_string(), "a".to_string())];
        let b = vec![("refs/heads/main".to_string(), "b".to_string())];
        assert!(!mismas_refs(&a, &b));
    }
}
