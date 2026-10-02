//! Cliente HTTP de la API de GitHub.

use std::time::Duration;

use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue, LINK, RETRY_AFTER};
use reqwest::{Client, Response, StatusCode, redirect};
use serde::de::DeserializeOwned;
use time::OffsetDateTime;
use url::Url;

use crate::modelo::{IdRepo, Nombre, RepoOrigen};
use crate::secretos::Secreto;

use super::caducidad;
use super::dto::{
    EstadoServicioDto, OrganizacionDto, RamaDto, RepoDto, UsuarioDto, convertir_repos,
};
use super::error::ErrorGithub;
use super::paginacion::{MAX_PAGINAS, mismo_origen, url_siguiente};
use super::{ApiGithub, EstadoServicio, Identidad};

const URL_API_POR_DEFECTO: &str = "https://api.github.com";
const URL_STATUS_POR_DEFECTO: &str = "https://www.githubstatus.com";
const TIEMPO_CONEXION: Duration = Duration::from_secs(10);
const TIEMPO_TOTAL: Duration = Duration::from_secs(30);
const ESPERA_BASE_POR_DEFECTO: Duration = Duration::from_millis(500);
const MAX_INTENTOS: u32 = 4;
const MAX_REDIRECCIONES: usize = 3;

/// Cliente de la API REST de GitHub sobre `reqwest`.
///
/// Mantiene dos clientes HTTP internos: uno con el token para la API de GitHub y otro
/// sin cabeceras de autenticación para `estado_servicio`, que consulta un servicio ajeno.
pub struct ClienteGithub {
    autenticado: Client,
    publico: Client,
    url_api: Url,
    url_status: Url,
    espera_base: Duration,
}

impl ClienteGithub {
    /// Cliente contra la API pública de GitHub (`api.github.com`).
    pub fn nuevo(token: Secreto) -> Result<Self, ErrorGithub> {
        Self::con_urls(token, URL_API_POR_DEFECTO, URL_STATUS_POR_DEFECTO)
    }

    /// Cliente con URLs base sustituibles, para pruebas contra un servidor simulado.
    ///
    /// Solo se permite `http` cuando el host es loopback (`127.0.0.1`, `::1` o
    /// `localhost`); cualquier otro uso de `http` se rechaza con
    /// [`ErrorGithub::UrlNoPermitida`].
    pub fn con_urls(token: Secreto, url_api: &str, url_status: &str) -> Result<Self, ErrorGithub> {
        let url_api = Url::parse(url_api).map_err(|_| ErrorGithub::UrlNoPermitida)?;
        let url_status = Url::parse(url_status).map_err(|_| ErrorGithub::UrlNoPermitida)?;
        comprobar_url_permitida(&url_api)?;
        comprobar_url_permitida(&url_status)?;

        let agente = format!("gitmereba/{}", env!("CARGO_PKG_VERSION"));

        let autenticado = Client::builder()
            .default_headers(cabeceras_autenticadas(&token)?)
            .user_agent(agente.clone())
            .connect_timeout(TIEMPO_CONEXION)
            .timeout(TIEMPO_TOTAL)
            .redirect(politica_mismo_host())
            .build()
            .map_err(|error| ErrorGithub::Red(error.without_url().to_string()))?;

        let publico = Client::builder()
            .user_agent(agente)
            .connect_timeout(TIEMPO_CONEXION)
            .timeout(TIEMPO_TOTAL)
            .redirect(politica_mismo_host())
            .build()
            .map_err(|error| ErrorGithub::Red(error.without_url().to_string()))?;

        Ok(Self {
            autenticado,
            publico,
            url_api,
            url_status,
            espera_base: ESPERA_BASE_POR_DEFECTO,
        })
    }

    /// Cambia la espera base de los reintentos exponenciales (por defecto 500 ms).
    ///
    /// Pensado para pruebas: reduce la espera a milisegundos.
    pub fn con_espera_base(mut self, base: Duration) -> Self {
        self.espera_base = base;
        self
    }

    fn url_con_segmentos(&self, segmentos: &[&str]) -> Result<Url, ErrorGithub> {
        let mut url = self.url_api.clone();
        url.path_segments_mut()
            .map_err(|()| ErrorGithub::UrlNoPermitida)?
            .extend(segmentos);
        Ok(url)
    }

    /// Envía una petición GET con reintentos y devuelve la respuesta si es un éxito o un
    /// 404 (para que las llamadas que lo tratan como «no existe» puedan distinguirlo).
    async fn get_con_reintentos(
        &self,
        cliente: &Client,
        url: Url,
    ) -> Result<Response, ErrorGithub> {
        let mut intento = 0u32;
        loop {
            intento += 1;
            match cliente.get(url.clone()).send().await {
                Ok(respuesta) => {
                    let estado = respuesta.status();
                    if estado.is_success() {
                        return Ok(respuesta);
                    }
                    let reintentable_por_estado =
                        estado == StatusCode::TOO_MANY_REQUESTS || estado.is_server_error();
                    if reintentable_por_estado && intento < MAX_INTENTOS {
                        tokio::time::sleep(espera_para(&respuesta, intento, self.espera_base))
                            .await;
                        continue;
                    }
                    return Err(respuesta_a_error(respuesta).await);
                }
                Err(error) => {
                    if intento < MAX_INTENTOS {
                        tokio::time::sleep(espera_exponencial(intento, self.espera_base)).await;
                        continue;
                    }
                    return Err(ErrorGithub::Red(error.without_url().to_string()));
                }
            }
        }
    }

    async fn paginar<T: DeserializeOwned>(&self, url_inicial: Url) -> Result<Vec<T>, ErrorGithub> {
        let mut resultado = Vec::new();
        let mut url_actual = Some(url_inicial);
        let mut pagina = 0u32;

        while let Some(url) = url_actual.take() {
            pagina += 1;
            if pagina > MAX_PAGINAS {
                break;
            }

            let respuesta = self.get_con_reintentos(&self.autenticado, url).await?;
            let siguiente = respuesta
                .headers()
                .get(LINK)
                .and_then(|valor| valor.to_str().ok())
                .and_then(url_siguiente);
            let elementos: Vec<T> = respuesta
                .json()
                .await
                .map_err(|error| ErrorGithub::DatosInvalidos(error.without_url().to_string()))?;
            resultado.extend(elementos);

            if let Some(siguiente) = siguiente {
                let siguiente = Url::parse(&siguiente).map_err(|_| {
                    ErrorGithub::DatosInvalidos("la paginación devolvió una URL inválida".into())
                })?;
                if !mismo_origen(&self.url_api, &siguiente) {
                    return Err(ErrorGithub::DatosInvalidos(
                        "la paginación apunta a un origen distinto del de la API".into(),
                    ));
                }
                url_actual = Some(siguiente);
            }
        }

        Ok(resultado)
    }
}

impl ApiGithub for ClienteGithub {
    async fn identidad(&self) -> Result<Identidad, ErrorGithub> {
        let url = self.url_con_segmentos(&["user"])?;
        let respuesta = self.get_con_reintentos(&self.autenticado, url).await?;

        let scopes = respuesta
            .headers()
            .get("x-oauth-scopes")
            .and_then(|valor| valor.to_str().ok())
            .map(|valor| {
                valor
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let caduca = match respuesta
            .headers()
            .get("github-authentication-token-expiration")
            .and_then(|valor| valor.to_str().ok())
        {
            Some(valor) => match caducidad::interpretar(valor) {
                Some(fecha) => Some(fecha),
                None => {
                    tracing::warn!(
                        "no se pudo interpretar la caducidad del token devuelta por GitHub"
                    );
                    None
                }
            },
            None => None,
        };

        let usuario: UsuarioDto = respuesta
            .json()
            .await
            .map_err(|error| ErrorGithub::DatosInvalidos(error.without_url().to_string()))?;
        let login = Nombre::nuevo(usuario.login)
            .map_err(|motivo| ErrorGithub::DatosInvalidos(motivo.to_string()))?;

        Ok(Identidad {
            login,
            scopes,
            caduca,
        })
    }

    async fn organizaciones(&self) -> Result<Vec<Nombre>, ErrorGithub> {
        let url = self.url_con_segmentos(&["user", "orgs"])?;
        let dtos: Vec<OrganizacionDto> = self.paginar(url).await?;
        Ok(dtos
            .into_iter()
            .filter_map(|dto| match Nombre::nuevo(dto.login.as_str()) {
                Ok(nombre) => Some(nombre),
                Err(motivo) => {
                    tracing::warn!(organizacion = %dto.login, %motivo, "organización omitida: nombre inválido");
                    None
                }
            })
            .collect())
    }

    async fn repos_de_usuario(&self) -> Result<Vec<RepoOrigen>, ErrorGithub> {
        let mut url = self.url_con_segmentos(&["user", "repos"])?;
        url.query_pairs_mut()
            .append_pair("affiliation", "owner")
            .append_pair("visibility", "all")
            .append_pair("per_page", "100");
        let dtos: Vec<RepoDto> = self.paginar(url).await?;
        Ok(convertir_repos(dtos))
    }

    async fn repos_de_organizacion(&self, org: &Nombre) -> Result<Vec<RepoOrigen>, ErrorGithub> {
        let mut url = self.url_con_segmentos(&["orgs", org.as_str(), "repos"])?;
        url.query_pairs_mut()
            .append_pair("type", "all")
            .append_pair("per_page", "100");
        let dtos: Vec<RepoDto> = self.paginar(url).await?;
        Ok(convertir_repos(dtos))
    }

    async fn sha_de_rama(&self, repo: &IdRepo, rama: &str) -> Result<Option<String>, ErrorGithub> {
        let mut url = self.url_con_segmentos(&[
            "repos",
            repo.dueno.as_str(),
            repo.nombre.as_str(),
            "branches",
        ])?;
        url.path_segments_mut()
            .map_err(|()| ErrorGithub::UrlNoPermitida)?
            .push(rama);

        match self.get_con_reintentos(&self.autenticado, url).await {
            Ok(respuesta) => {
                let dto: RamaDto = respuesta.json().await.map_err(|error| {
                    ErrorGithub::DatosInvalidos(error.without_url().to_string())
                })?;
                Ok(Some(dto.commit.sha))
            }
            Err(ErrorGithub::NoEncontrado) => Ok(None),
            Err(otro) => Err(otro),
        }
    }

    async fn estado_servicio(&self) -> Result<EstadoServicio, ErrorGithub> {
        let mut url = self.url_status.clone();
        url.path_segments_mut()
            .map_err(|()| ErrorGithub::UrlNoPermitida)?
            .extend(["api", "v2", "status.json"]);

        let respuesta = self.get_con_reintentos(&self.publico, url).await?;
        let dto: EstadoServicioDto = respuesta
            .json()
            .await
            .map_err(|error| ErrorGithub::DatosInvalidos(error.without_url().to_string()))?;

        Ok(match dto.status.indicator.as_str() {
            "none" => EstadoServicio::Operativo,
            "minor" => EstadoServicio::Degradado,
            "major" | "critical" => EstadoServicio::Caido,
            _ => EstadoServicio::Desconocido,
        })
    }
}

/// Cabeceras por defecto del cliente autenticado.
fn cabeceras_autenticadas(token: &Secreto) -> Result<HeaderMap, ErrorGithub> {
    let mut cabeceras = HeaderMap::new();

    let mut valor_token =
        HeaderValue::from_str(&format!("Bearer {}", token.exponer())).map_err(|_| {
            ErrorGithub::DatosInvalidos(
                "el token contiene caracteres no válidos para una cabecera".into(),
            )
        })?;
    valor_token.set_sensitive(true);
    cabeceras.insert(AUTHORIZATION, valor_token);

    cabeceras.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );
    cabeceras.insert(
        HeaderName::from_static("x-github-api-version"),
        HeaderValue::from_static("2022-11-28"),
    );

    Ok(cabeceras)
}

/// Solo se permite `https`, o `http` cuando el host es loopback (para pruebas locales).
fn comprobar_url_permitida(url: &Url) -> Result<(), ErrorGithub> {
    match url.scheme() {
        "https" => Ok(()),
        "http" if es_host_loopback(url) => Ok(()),
        _ => Err(ErrorGithub::UrlNoPermitida),
    }
}

fn es_host_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(dominio)) => dominio.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// Máximo `MAX_REDIRECCIONES` saltos, y solo si el host coincide con el de la petición
/// original.
fn politica_mismo_host() -> redirect::Policy {
    redirect::Policy::custom(|intento| {
        if intento.previous().len() >= MAX_REDIRECCIONES {
            return intento.error("se ha superado el máximo de redirecciones permitidas");
        }
        let host_original = intento.previous().first().and_then(|url| url.host_str());
        if host_original == intento.url().host_str() {
            intento.follow()
        } else {
            intento.error("redirección a un host distinto del original, no permitida")
        }
    })
}

/// Tope de cualquier espera entre reintentos: un `retry-after` enorme no bloquea la app.
const ESPERA_MAXIMA: Duration = Duration::from_secs(60);

/// Espera antes de reintentar: respeta `retry-after` si está presente; si no, exponencial.
/// En ambos casos con el tope de [`ESPERA_MAXIMA`].
fn espera_para(respuesta: &Response, intento: u32, base: Duration) -> Duration {
    if let Some(segundos) = respuesta
        .headers()
        .get(RETRY_AFTER)
        .and_then(|valor| valor.to_str().ok())
        .and_then(|valor| valor.trim().parse::<u64>().ok())
    {
        return Duration::from_secs(segundos).min(ESPERA_MAXIMA);
    }
    espera_exponencial(intento, base).min(ESPERA_MAXIMA)
}

fn espera_exponencial(intento: u32, base: Duration) -> Duration {
    base.saturating_mul(2u32.saturating_pow(intento.saturating_sub(1)))
}

/// Traduce una respuesta que no es un éxito a un [`ErrorGithub`].
async fn respuesta_a_error(respuesta: Response) -> ErrorGithub {
    match respuesta.status() {
        StatusCode::UNAUTHORIZED => ErrorGithub::TokenInvalido,
        StatusCode::FORBIDDEN => {
            let limite_agotado = respuesta
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|valor| valor.to_str().ok())
                == Some("0");
            if limite_agotado {
                let reinicio = respuesta
                    .headers()
                    .get("x-ratelimit-reset")
                    .and_then(|valor| valor.to_str().ok())
                    .and_then(|valor| valor.parse::<i64>().ok())
                    .and_then(|epoch| OffsetDateTime::from_unix_timestamp(epoch).ok());
                ErrorGithub::LimiteDePeticiones { reinicio }
            } else {
                ErrorGithub::SinPermiso
            }
        }
        StatusCode::NOT_FOUND => ErrorGithub::NoEncontrado,
        estado => {
            let detalle = respuesta.text().await.unwrap_or_default();
            ErrorGithub::RespuestaInesperada {
                estado: estado.as_u16(),
                detalle,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::modelo::IdRepo;

    use super::*;

    const TOKEN: &str = "ghp_test_token_secreto";

    fn secreto() -> Secreto {
        Secreto::nuevo(TOKEN)
    }

    fn cliente_de(servidor: &MockServer) -> ClienteGithub {
        ClienteGithub::con_urls(secreto(), &servidor.uri(), &servidor.uri())
            .unwrap()
            .con_espera_base(Duration::from_millis(1))
    }

    #[tokio::test]
    async fn identidad_con_scopes_y_caducidad_en_formato_utc() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"login": "jparga"}))
                    .insert_header("x-oauth-scopes", "repo, read:org")
                    .insert_header(
                        "github-authentication-token-expiration",
                        "2026-12-01 10:00:00 UTC",
                    ),
            )
            .mount(&servidor)
            .await;

        let identidad = cliente_de(&servidor).identidad().await.unwrap();

        assert_eq!(identidad.login.as_str(), "jparga");
        assert_eq!(
            identidad.scopes,
            vec!["repo".to_string(), "read:org".to_string()]
        );
        assert!(identidad.caduca.is_some());
    }

    #[tokio::test]
    async fn identidad_con_caducidad_en_formato_con_desplazamiento() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"login": "jparga"}))
                    .insert_header(
                        "github-authentication-token-expiration",
                        "2026-12-01 10:00:00 +0100",
                    ),
            )
            .mount(&servidor)
            .await;

        let identidad = cliente_de(&servidor).identidad().await.unwrap();

        assert!(identidad.caduca.is_some());
    }

    #[tokio::test]
    async fn identidad_sin_scopes_ni_caducidad_token_fine_grained() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login": "jparga"})))
            .mount(&servidor)
            .await;

        let identidad = cliente_de(&servidor).identidad().await.unwrap();

        assert!(identidad.scopes.is_empty());
        assert_eq!(identidad.caduca, None);
    }

    #[tokio::test]
    async fn identidad_401_da_token_invalido() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&servidor)
            .await;

        let error = cliente_de(&servidor).identidad().await.unwrap_err();

        assert_eq!(error, ErrorGithub::TokenInvalido);
    }

    #[tokio::test]
    async fn identidad_403_con_limite_agotado_da_limite_de_peticiones() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("x-ratelimit-remaining", "0")
                    .insert_header("x-ratelimit-reset", "1900000000"),
            )
            .mount(&servidor)
            .await;

        let error = cliente_de(&servidor).identidad().await.unwrap_err();

        match error {
            ErrorGithub::LimiteDePeticiones { reinicio } => {
                assert_eq!(
                    reinicio,
                    OffsetDateTime::from_unix_timestamp(1_900_000_000).ok()
                );
            }
            otro => panic!("se esperaba LimiteDePeticiones, llegó {otro:?}"),
        }
    }

    #[tokio::test]
    async fn identidad_403_sin_limite_agotado_da_sin_permiso() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(403).insert_header("x-ratelimit-remaining", "42"))
            .mount(&servidor)
            .await;

        let error = cliente_de(&servidor).identidad().await.unwrap_err();

        assert_eq!(error, ErrorGithub::SinPermiso);
    }

    #[tokio::test]
    async fn sigue_la_paginacion_de_tres_paginas_por_cabecera_link() {
        let servidor = MockServer::start().await;
        let base = servidor.uri();

        Mock::given(method("GET"))
            .and(path("/user/orgs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([{"login": "org1"}]))
                    .insert_header("Link", format!("<{base}/user/orgs/pagina-2>; rel=\"next\"")),
            )
            .mount(&servidor)
            .await;
        Mock::given(method("GET"))
            .and(path("/user/orgs/pagina-2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([{"login": "org2"}]))
                    .insert_header("Link", format!("<{base}/user/orgs/pagina-3>; rel=\"next\"")),
            )
            .mount(&servidor)
            .await;
        Mock::given(method("GET"))
            .and(path("/user/orgs/pagina-3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"login": "org3"}])))
            .mount(&servidor)
            .await;

        let orgs = cliente_de(&servidor).organizaciones().await.unwrap();

        assert_eq!(
            orgs.iter().map(Nombre::as_str).collect::<Vec<_>>(),
            vec!["org1", "org2", "org3"]
        );
    }

    #[tokio::test]
    async fn la_paginacion_no_sigue_un_link_a_otro_origen() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user/orgs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([{"login": "org1"}]))
                    .insert_header(
                        "Link",
                        "<http://otro-host.invalido/siguiente>; rel=\"next\"",
                    ),
            )
            .mount(&servidor)
            .await;

        let error = cliente_de(&servidor).organizaciones().await.unwrap_err();

        assert!(matches!(error, ErrorGithub::DatosInvalidos(_)));
        let peticiones = servidor.received_requests().await.unwrap();
        assert_eq!(peticiones.len(), 1);
    }

    #[tokio::test]
    async fn reintenta_tras_un_502_y_luego_acierta() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(502))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&servidor)
            .await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login": "jparga"})))
            .with_priority(2)
            .mount(&servidor)
            .await;

        let identidad = cliente_de(&servidor).identidad().await.unwrap();

        assert_eq!(identidad.login.as_str(), "jparga");
        let peticiones = servidor.received_requests().await.unwrap();
        assert_eq!(peticiones.len(), 2);
    }

    #[tokio::test]
    async fn respeta_retry_after_en_un_429() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&servidor)
            .await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login": "jparga"})))
            .with_priority(2)
            .mount(&servidor)
            .await;

        let identidad = cliente_de(&servidor).identidad().await.unwrap();

        assert_eq!(identidad.login.as_str(), "jparga");
        let peticiones = servidor.received_requests().await.unwrap();
        assert_eq!(peticiones.len(), 2);
    }

    #[tokio::test]
    async fn sha_de_rama_codifica_la_rama_como_segmento_de_ruta() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/jparga/gitmereba/branches/feature%2Fx%20y"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"commit": {"sha": "abc123"}})),
            )
            .mount(&servidor)
            .await;

        let repo = IdRepo {
            dueno: Nombre::nuevo("jparga").unwrap(),
            nombre: Nombre::nuevo("gitmereba").unwrap(),
        };
        let sha = cliente_de(&servidor)
            .sha_de_rama(&repo, "feature/x y")
            .await
            .unwrap();

        assert_eq!(sha, Some("abc123".to_string()));
    }

    #[tokio::test]
    async fn sha_de_rama_404_da_none() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&servidor)
            .await;

        let repo = IdRepo {
            dueno: Nombre::nuevo("jparga").unwrap(),
            nombre: Nombre::nuevo("gitmereba").unwrap(),
        };
        let sha = cliente_de(&servidor)
            .sha_de_rama(&repo, "no-existe")
            .await
            .unwrap();

        assert_eq!(sha, None);
    }

    #[tokio::test]
    async fn estado_servicio_interpreta_los_cuatro_indicadores() {
        for (indicador, esperado) in [
            ("none", EstadoServicio::Operativo),
            ("minor", EstadoServicio::Degradado),
            ("major", EstadoServicio::Caido),
            ("critical", EstadoServicio::Caido),
        ] {
            let servidor = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v2/status.json"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({"status": {"indicator": indicador}})),
                )
                .mount(&servidor)
                .await;

            let estado = cliente_de(&servidor).estado_servicio().await.unwrap();
            assert_eq!(estado, esperado, "indicador {indicador}");

            let peticiones = servidor.received_requests().await.unwrap();
            assert!(!peticiones[0].headers.contains_key("authorization"));
        }
    }

    #[tokio::test]
    async fn se_envian_las_cabeceras_esperadas() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login": "jparga"})))
            .mount(&servidor)
            .await;

        cliente_de(&servidor).identidad().await.unwrap();

        let peticiones = servidor.received_requests().await.unwrap();
        let cabeceras = &peticiones[0].headers;
        assert_eq!(
            cabeceras.get("authorization").unwrap(),
            &format!("Bearer {TOKEN}")
        );
        assert_eq!(cabeceras.get("x-github-api-version").unwrap(), "2022-11-28");
        assert!(
            cabeceras
                .get("user-agent")
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("gitmereba/")
        );
    }

    #[tokio::test]
    async fn el_error_de_conexion_no_expone_el_token() {
        // Puerto cerrado: se reserva y se libera antes de usarlo, para que la conexión
        // se rechace de forma predecible.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let puerto = listener.local_addr().unwrap().port();
        drop(listener);
        let url = format!("http://127.0.0.1:{puerto}");

        let cliente = ClienteGithub::con_urls(secreto(), &url, &url)
            .unwrap()
            .con_espera_base(Duration::from_millis(1));

        let error = cliente.identidad().await.unwrap_err();
        let depurado = format!("{error:?}");
        let mostrado = error.to_string();

        assert!(!depurado.contains(TOKEN));
        assert!(!mostrado.contains(TOKEN));
        assert!(matches!(error, ErrorGithub::Red(_)));
    }

    #[test]
    fn el_constructor_rechaza_http_a_un_host_no_local() {
        let error = ClienteGithub::con_urls(
            secreto(),
            "http://ejemplo.com",
            "https://www.githubstatus.com",
        );

        assert_eq!(error.err(), Some(ErrorGithub::UrlNoPermitida));
    }

    #[test]
    fn la_espera_exponencial_crece_y_tiene_tope() {
        let base = Duration::from_millis(500);
        assert_eq!(espera_exponencial(1, base), Duration::from_millis(500));
        assert_eq!(espera_exponencial(3, base), Duration::from_secs(2));
        assert_eq!(
            espera_exponencial(40, base).min(ESPERA_MAXIMA),
            ESPERA_MAXIMA
        );
    }

    #[tokio::test]
    async fn un_retry_after_enorme_se_recorta_al_tope() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "86400"))
            .mount(&servidor)
            .await;
        let respuesta = reqwest::get(servidor.uri()).await.unwrap();
        assert_eq!(
            espera_para(&respuesta, 1, Duration::from_millis(1)),
            ESPERA_MAXIMA
        );
    }
}
