//! Comprobaciones de la interfaz traducible (sin Node): los diccionarios de `ui/i18n/`
//! tienen las mismas claves, toda clave usada existe y no quedan textos sueltos en `ui/js/`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Vistas y ficheros de `ui/js/` que aún tienen textos en español sueltos. Se van
/// vaciando a medida que cada vista se migra a `t()`; la lista vacía es el criterio de
/// cierre. Un fichero de esta lista que ya está limpio hace fallar el test: hay que
/// quitarlo.
const PENDIENTES: &[&str] = &["ayuda.js", "vistas/ajustes.js"];

/// Nunca se revisan: `mock.js` son datos de ejemplo para previsualizar en un navegador y
/// no forman parte de la ventana real.
const FUERA_DE_REVISION: &[&str] = &["mock.js"];

/// Literales con letras que no se traducen (marcas y nombres propios).
/// Incluye las unidades de tamaño, iguales en los dos idiomas.
const EXCEPCIONES: &[&str] = &["MEREBA", "GitHub", "Gitea", "KB", "MB", "GB"];

const ACENTOS: &str = "áéíóúñüÁÉÍÓÚÑÜ¿¡«»";

fn ui() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui")
}

fn leer(ruta: &Path) -> String {
    fs::read_to_string(ruta).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", ruta.display()))
}

fn ficheros_js(dir: &Path, base: &Path, salida: &mut Vec<(String, PathBuf)>) {
    let mut entradas: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("no se pudo listar {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entradas.sort();
    for ruta in entradas {
        if ruta.is_dir() {
            ficheros_js(&ruta, base, salida);
        } else if ruta.extension().is_some_and(|e| e == "js") {
            let relativa = ruta
                .strip_prefix(base)
                .map(|r| r.to_string_lossy().into_owned())
                .unwrap_or_default();
            salida.push((relativa, ruta));
        }
    }
}

fn todos_los_js() -> Vec<(String, PathBuf)> {
    let base = ui().join("js");
    let mut salida = Vec::new();
    ficheros_js(&base, &base, &mut salida);
    salida
}

// ---------- Diccionarios ----------

/// Claves de un diccionario (formato documentado en la cabecera de `ui/i18n/es.js`).
/// Dentro de `export default { … }` toda línea debe ser vacía, comentario, cierre o una
/// entrada `'clave': 'valor',`; cualquier otra devuelve error con su número de línea.
fn claves_de(texto: &str) -> Result<Vec<String>, String> {
    let mut claves = Vec::new();
    let mut dentro = false;
    for (n, linea) in texto.lines().enumerate() {
        let t = linea.trim();
        if !dentro {
            dentro = t.starts_with("export default {");
            continue;
        }
        if t.is_empty() || t.starts_with("//") || t == "}" || t == "};" {
            continue;
        }
        let entrada = t
            .strip_prefix('\'')
            .and_then(|resto| resto.split_once("': '"));
        match entrada {
            Some((clave, valor))
                if !clave.is_empty()
                    && !clave.contains(['\\', '\'', '"'])
                    && valor.ends_with("',") =>
            {
                claves.push(clave.to_string());
            }
            _ => {
                return Err(format!(
                    "línea {}: no cumple el formato `'clave': 'valor',`: {t}",
                    n + 1
                ));
            }
        }
    }
    if !dentro {
        return Err("falta `export default {`".to_string());
    }
    Ok(claves)
}

fn diccionario(idioma: &str) -> Vec<String> {
    let ruta = ui().join(format!("i18n/{idioma}.js"));
    claves_de(&leer(&ruta)).unwrap_or_else(|e| panic!("{}: {e}", ruta.display()))
}

#[test]
fn los_diccionarios_tienen_las_mismas_claves() {
    let es = diccionario("es");
    let en = diccionario("en");
    assert!(!es.is_empty(), "es.js no tiene claves");
    for (nombre, claves) in [("es.js", &es), ("en.js", &en)] {
        let unicas: BTreeSet<_> = claves.iter().collect();
        assert_eq!(unicas.len(), claves.len(), "{nombre} repite alguna clave");
    }
    let es: BTreeSet<_> = es.into_iter().collect();
    let en: BTreeSet<_> = en.into_iter().collect();
    let solo_es: Vec<_> = es.difference(&en).collect();
    let solo_en: Vec<_> = en.difference(&es).collect();
    assert!(
        solo_es.is_empty() && solo_en.is_empty(),
        "claves solo en es.js: {solo_es:?}; solo en en.js: {solo_en:?}"
    );
}

// ---------- Análisis ligero de JS ----------

#[derive(Debug)]
struct Literal {
    inicio: usize,
    fin: usize,
    /// Contenido sin comillas; en una plantilla, solo las partes fuera de `${…}`.
    texto: String,
}

/// Devuelve el código con los comentarios convertidos en espacios (mismas posiciones) y
/// los literales de cadena encontrados.
fn analizar(fuente: &str) -> (Vec<char>, Vec<Literal>) {
    let mut c: Vec<char> = fuente.chars().collect();
    let mut literales = Vec::new();
    let mut i = 0;
    while i < c.len() {
        match c[i] {
            '/' if c.get(i + 1) == Some(&'/') => {
                while i < c.len() && c[i] != '\n' {
                    c[i] = ' ';
                    i += 1;
                }
            }
            '/' if c.get(i + 1) == Some(&'*') => {
                while i < c.len() && !(c[i] == '*' && c.get(i + 1) == Some(&'/')) {
                    if c[i] != '\n' {
                        c[i] = ' ';
                    }
                    i += 1;
                }
                for k in [i, i + 1] {
                    if k < c.len() {
                        c[k] = ' ';
                    }
                }
                i += 2;
            }
            '\'' | '"' | '`' => {
                let (fin, texto) = leer_literal(&c, i);
                literales.push(Literal {
                    inicio: i,
                    fin,
                    texto,
                });
                i = fin;
            }
            _ => i += 1,
        }
    }
    (c, literales)
}

/// Lee el literal que empieza en `inicio`; devuelve la posición siguiente a su cierre y
/// su texto estático.
fn leer_literal(c: &[char], inicio: usize) -> (usize, String) {
    let comilla = c[inicio];
    let mut i = inicio + 1;
    let mut texto = String::new();
    while i < c.len() {
        match c[i] {
            '\\' => {
                texto.push(c.get(i + 1).copied().unwrap_or(' '));
                i += 2;
            }
            ch if ch == comilla => return (i + 1, texto),
            '$' if comilla == '`' && c.get(i + 1) == Some(&'{') => {
                texto.push(' ');
                i += 2;
                let mut profundidad = 1;
                while i < c.len() && profundidad > 0 {
                    match c[i] {
                        '{' => profundidad += 1,
                        '}' => profundidad -= 1,
                        '\'' | '"' | '`' => {
                            i = leer_literal(c, i).0;
                            continue;
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            ch => {
                texto.push(ch);
                i += 1;
            }
        }
    }
    (c.len(), texto)
}

fn es_identificador(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '$'
}

fn linea_de(c: &[char], pos: usize) -> usize {
    c[..pos].iter().filter(|&&ch| ch == '\n').count() + 1
}

fn anterior_no_blanco(c: &[char], pos: usize) -> Option<usize> {
    (0..pos).rev().find(|&k| !c[k].is_whitespace())
}

/// Claves pasadas a `t('…')` como literal.
fn claves_usadas_en_js(fuente: &str) -> Vec<(usize, String)> {
    let (c, literales) = analizar(fuente);
    let mut usadas = Vec::new();
    for (i, ch) in c.iter().enumerate() {
        if *ch != 't' || c.get(i + 1) != Some(&'(') {
            continue;
        }
        if i > 0 && (es_identificador(c[i - 1]) || c[i - 1] == '.') {
            continue;
        }
        if let Some(lit) = literales.iter().find(|l| {
            let siguiente = (i + 2..c.len()).find(|&k| !c[k].is_whitespace());
            siguiente == Some(l.inicio)
        }) {
            usadas.push((linea_de(&c, i), lit.texto.clone()));
        }
    }
    usadas
}

/// Claves de `data-i18n="…"`, `data-i18n-title="…"` y `data-i18n-aria-label="…"`.
fn claves_usadas_en_html(fuente: &str) -> Vec<(usize, String)> {
    let mut usadas = Vec::new();
    for (n, linea) in fuente.lines().enumerate() {
        let mut resto = linea;
        while let Some(p) = resto.find("data-i18n") {
            resto = &resto[p + "data-i18n".len()..];
            let Some(igual) = resto.find("=\"") else {
                break;
            };
            let atributo = &resto[..igual];
            if !["", "-title", "-aria-label"].contains(&atributo) {
                continue;
            }
            let valor = &resto[igual + 2..];
            let Some(fin) = valor.find('"') else { break };
            usadas.push((n + 1, valor[..fin].to_string()));
            resto = &valor[fin..];
        }
    }
    usadas
}

fn existe_clave(claves: &BTreeSet<String>, clave: &str) -> bool {
    claves.contains(clave)
        || (claves.contains(&format!("{clave}_one")) && claves.contains(&format!("{clave}_other")))
}

#[test]
fn toda_clave_usada_existe_en_los_diccionarios() {
    let claves: BTreeSet<String> = diccionario("es").into_iter().collect();
    let mut faltan = Vec::new();
    let html = ui().join("index.html");
    for (linea, clave) in claves_usadas_en_html(&leer(&html)) {
        if !existe_clave(&claves, &clave) {
            faltan.push(format!("index.html:{linea}: {clave}"));
        }
    }
    for (nombre, ruta) in todos_los_js() {
        for (linea, clave) in claves_usadas_en_js(&leer(&ruta)) {
            if !existe_clave(&claves, &clave) {
                faltan.push(format!("{nombre}:{linea}: {clave}"));
            }
        }
    }
    assert!(
        faltan.is_empty(),
        "claves que no existen en es.js:\n{}",
        faltan.join("\n")
    );
}

// ---------- Cadenas sueltas ----------

fn tiene_letras(texto: &str) -> bool {
    texto.chars().any(char::is_alphabetic)
}

/// Literales de `fuente` que parecen texto visible sin traducir.
fn cadenas_sueltas(fuente: &str) -> Vec<String> {
    let (c, literales) = analizar(fuente);
    let mut sueltas: Vec<(usize, String)> = Vec::new();
    let marcar = |sueltas: &mut Vec<(usize, String)>, lit: &Literal, motivo: &str| {
        let texto = lit.texto.trim();
        if !EXCEPCIONES.contains(&texto) && !sueltas.iter().any(|(inicio, _)| *inicio == lit.inicio)
        {
            sueltas.push((
                lit.inicio,
                format!("línea {}: «{texto}» ({motivo})", linea_de(&c, lit.inicio)),
            ));
        }
    };

    for lit in &literales {
        let texto = lit.texto.trim();
        if texto.chars().any(|ch| ACENTOS.contains(ch)) {
            marcar(&mut sueltas, lit, "tildes o «»");
            continue;
        }
        if !tiene_letras(texto) {
            continue;
        }
        // `textContent = '…'` / `+= '…'`
        if let Some(mut k) = anterior_no_blanco(&c, lit.inicio) {
            if c[k] == '=' && c.get(k.wrapping_sub(1)) != Some(&'=') {
                k = k.saturating_sub(1);
                if c[k] == '+' {
                    k = k.saturating_sub(1);
                }
                let antes: String = c[..=k].iter().collect();
                if antes.trim_end().ends_with("textContent") {
                    marcar(&mut sueltas, lit, "textContent");
                    continue;
                }
            }
            // Valor de propiedad que es una palabra con mayúscula inicial (`fallo: 'Fallo'`).
            if c[k] == ':' && texto.chars().next().is_some_and(char::is_uppercase) {
                marcar(&mut sueltas, lit, "valor de propiedad");
                continue;
            }
            // `title: '…'`, `placeholder: '…'`, `'aria-label': '…'`
            if c[k] == ':' {
                let antes: String = c[..k].iter().collect();
                let antes = antes.trim_end().trim_end_matches(['\'', '"']);
                if ["title", "placeholder", "alt", "aria-label"]
                    .iter()
                    .any(|a| antes.ends_with(a))
                {
                    marcar(&mut sueltas, lit, "atributo visible");
                    continue;
                }
            }
        }
        // Frase: empieza en mayúscula y tiene espacios.
        if texto.contains(' ') && texto.chars().next().is_some_and(char::is_uppercase) {
            marcar(&mut sueltas, lit, "frase");
        }
    }

    // Llamadas cuyo argumento literal es texto visible:
    //   `h('etiqueta', attrs, 'texto', …)`: hijos (índice 2 o más);
    //   `avisar('texto', …)`: primer argumento;
    //   `x.setAttribute('title'|'aria-label'|'placeholder', 'texto')`: segundo argumento.
    for i in 0..c.len() {
        let antes_ok = i == 0 || !(es_identificador(c[i - 1]) || c[i - 1] == '.');
        let empieza = |nombre: &str| {
            c[i..].starts_with(&nombre.chars().collect::<Vec<_>>())
                && c.get(i + nombre.chars().count()) == Some(&'(')
        };
        if antes_ok && empieza("h") {
            for (indice, lit) in argumentos_literales(&c, &literales, i + 2) {
                if indice >= 2 && tiene_letras(&lit.texto) {
                    marcar(&mut sueltas, lit, "hijo de h()");
                }
            }
        } else if antes_ok && empieza("avisar") {
            for (indice, lit) in argumentos_literales(&c, &literales, i + 7) {
                if indice == 0 && tiene_letras(&lit.texto) {
                    marcar(&mut sueltas, lit, "texto de avisar()");
                }
            }
        } else if empieza("setAttribute") && i > 0 && c[i - 1] == '.' {
            let args = argumentos_literales(&c, &literales, i + 13);
            let atributo_visible = args.iter().any(|(indice, lit)| {
                *indice == 0 && ["title", "aria-label", "placeholder"].contains(&lit.texto.as_str())
            });
            if atributo_visible {
                for (indice, lit) in args {
                    if indice == 1 && tiene_letras(&lit.texto) {
                        marcar(&mut sueltas, lit, "atributo visible de setAttribute()");
                    }
                }
            }
        }
    }
    sueltas.into_iter().map(|(_, texto)| texto).collect()
}

/// Argumentos de una llamada (desde `inicio`, tras el paréntesis) que son exactamente un
/// literal, con su posición en la lista de argumentos.
fn argumentos_literales<'a>(
    c: &[char],
    literales: &'a [Literal],
    inicio: usize,
) -> Vec<(usize, &'a Literal)> {
    let mut resultado = Vec::new();
    let mut profundidad = 0;
    let mut indice = 0;
    let mut inicio_arg = inicio;
    let mut i = inicio;
    while i < c.len() {
        if let Some(lit) = literales.iter().find(|l| l.inicio == i) {
            i = lit.fin;
            continue;
        }
        let cierra_arg = match c[i] {
            '(' | '[' | '{' => {
                profundidad += 1;
                false
            }
            ')' | ']' | '}' if profundidad == 0 => true,
            ')' | ']' | '}' => {
                profundidad -= 1;
                false
            }
            ',' if profundidad == 0 => true,
            _ => false,
        };
        if cierra_arg {
            {
                let primero = (inicio_arg..i).find(|&k| !c[k].is_whitespace());
                if let Some(lit) = primero.and_then(|p| literales.iter().find(|l| l.inicio == p)) {
                    let resto_vacio = c[lit.fin..i].iter().all(|ch| ch.is_whitespace());
                    if resto_vacio {
                        resultado.push((indice, lit));
                    }
                }
            }
            if c[i] != ',' {
                break;
            }
            indice += 1;
            inicio_arg = i + 1;
        }
        i += 1;
    }
    resultado
}

#[test]
fn el_detector_de_cadenas_sueltas_funciona() {
    let mala = "h('p', null, 'Hola mundo');\nel.textContent = 'Listo';\nconst x = 'canción';\n// 'Comentario con tildes é'\n";
    let sueltas = cadenas_sueltas(mala);
    assert_eq!(sueltas.len(), 3, "{sueltas:?}");
    let buena = "h('p', { clase: 'btn ghost' }, t('a.b'), `${n} KB`);\nel.textContent = t('a.c');\nconst u = 'http://x';\n";
    assert!(
        cadenas_sueltas(buena).is_empty(),
        "{:?}",
        cadenas_sueltas(buena)
    );
    assert!(cadenas_sueltas("avisar('Hecho', 'success');").len() == 1);
    assert!(cadenas_sueltas("avisar(t('a.b'), 'success');").is_empty());
    assert!(cadenas_sueltas("el.setAttribute('aria-label', 'Cerrar');").len() == 1);
    assert!(cadenas_sueltas("el.setAttribute('aria-busy', 'true');").is_empty());
    assert!(cadenas_sueltas("el.setAttribute('title', t('a.b'));").is_empty());
    assert_eq!(
        claves_usadas_en_js("x(t('a.b'), at('no'), o.t('no'), t(clave))").len(),
        1
    );
    assert_eq!(
        claves_usadas_en_html("<a data-i18n=\"x.y\" data-i18n-title=\"x.z\" data-ruta=\"r\">")
            .len(),
        2
    );
}

#[test]
fn no_quedan_cadenas_sueltas_fuera_de_la_lista_de_pendientes() {
    let mut con_sueltas = Vec::new();
    for (nombre, ruta) in todos_los_js() {
        if FUERA_DE_REVISION.contains(&nombre.as_str()) {
            continue;
        }
        let sueltas = cadenas_sueltas(&leer(&ruta));
        let pendiente = PENDIENTES.contains(&nombre.as_str());
        if pendiente && sueltas.is_empty() {
            con_sueltas.push(format!("{nombre}: ya está migrado; quítalo de PENDIENTES"));
        } else if !pendiente {
            con_sueltas.extend(sueltas.into_iter().map(|s| format!("{nombre}: {s}")));
        }
    }
    assert!(
        con_sueltas.is_empty(),
        "cadenas sueltas:\n{}",
        con_sueltas.join("\n")
    );
}

#[test]
fn la_lista_de_pendientes_solo_nombra_ficheros_que_existen() {
    for nombre in PENDIENTES.iter().chain(FUERA_DE_REVISION) {
        assert!(
            ui().join("js").join(nombre).is_file(),
            "no existe ui/js/{nombre}"
        );
    }
}

#[test]
fn el_analizador_de_diccionarios_rechaza_lo_que_no_cumple_el_formato() {
    let bien = "// c\nexport default {\n  'a.b': 'x',\n  'a.c': 'it\\'s',\n};\n";
    assert_eq!(claves_de(bien).unwrap(), ["a.b", "a.c"]);
    for mala in [
        "\"x\": 'y',",
        "'x': \"y\",",
        "'a'b': 'y',",
        "'x': 'y'",
        "otra cosa",
    ] {
        let texto = format!("export default {{\n  'ok.k': 'v',\n  {mala}\n}};\n");
        let error = claves_de(&texto).expect_err(mala);
        assert!(error.starts_with("línea 3"), "{error}");
    }
}
