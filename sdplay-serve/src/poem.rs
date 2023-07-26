use poem::{listener::TcpListener, web::Data, EndpointExt, Result, Route};
use poem_openapi::{
    payload::{Json, PlainText},
    Object, OpenApi, OpenApiService,
};
use sdplay_lib::{
    audio::play,
    error::ToSdpPlayerResult,
    sdp::{session_descriptor_from_sdp_str, session_descriptor_from_sdp_url},
    stream::Stream,
    SessionDescriptor,
};
use std::{net::Ipv4Addr, time::Duration};
use tokio::{spawn, sync::broadcast, time::sleep};
use url::Url;

struct Api;

#[derive(Debug, Clone, Object)]
pub struct Status {
    playing: bool,
}

#[OpenApi]
impl Api {
    #[oai(path = "/play/descriptor", method = "post")]
    async fn play_sd(
        &self,
        Data(stop): Data<&broadcast::Sender<()>>,
        Json(sd): Json<SessionDescriptor>,
    ) -> Result<Json<&'static str>> {
        stop.send(()).convert()?;
        sleep(Duration::from_millis(100)).await;

        log::info!("Playing SessionDescriptor from URL: {sd:?}");

        let local_address = Ipv4Addr::UNSPECIFIED;
        let stream = Stream::new(sd, local_address).await?;
        spawn(play(stream, stop.clone()));

        Ok(Json("Ok"))
    }

    #[oai(path = "/play/url", method = "post")]
    async fn play_url(
        &self,
        Data(stop): Data<&broadcast::Sender<()>>,
        Json(url): Json<Url>,
    ) -> Result<Json<&'static str>> {
        stop.send(()).convert()?;
        sleep(Duration::from_millis(100)).await;

        log::info!("Playing SDP from URL: {url}");

        let local_address = Ipv4Addr::UNSPECIFIED;
        let sd = session_descriptor_from_sdp_url(&url).await?;
        let stream = Stream::new(sd, local_address).await?;
        spawn(play(stream, stop.clone()));

        Ok(Json("Ok"))
    }

    #[oai(path = "/play/sdp", method = "post")]
    async fn play_sdp(
        &self,
        Data(stop): Data<&broadcast::Sender<()>>,
        PlainText(sdp): PlainText<String>,
    ) -> Result<Json<&'static str>> {
        stop.send(()).convert()?;
        sleep(Duration::from_millis(100)).await;

        log::info!("Playing SDP: {sdp}");

        let local_address = Ipv4Addr::UNSPECIFIED;
        let sd = session_descriptor_from_sdp_str(&sdp).await?;
        let stream = Stream::new(sd, local_address).await?;
        spawn(play(stream, stop.clone()));

        Ok(Json("Ok"))
    }

    #[oai(path = "/status", method = "get")]
    async fn status(&self) -> Result<Json<Status>> {
        log::info!("Getting status");
        // TODO
        Ok(Json(Status { playing: true }))
    }

    #[oai(path = "/stop", method = "post")]
    async fn stop(&self, Data(stop): Data<&broadcast::Sender<()>>) -> Result<Json<&'static str>> {
        log::info!("Stopping receiver");
        stop.send(()).convert()?;
        Ok(Json("Ok"))
    }

    #[oai(path = "/volume", method = "get")]
    async fn get_volume(&self) -> Result<Json<f32>> {
        log::info!("Getting volume");
        // TODO
        Ok(Json(0.5))
    }

    #[oai(path = "/volume/set", method = "post")]
    async fn set_volume(&self, Json(volume): Json<f32>) -> Result<Json<&'static str>> {
        log::info!("Setting volume to: {volume}");
        // TODO
        Ok(Json("Ok"))
    }
}

pub async fn start() -> anyhow::Result<()> {
    let public_addr = Ipv4Addr::LOCALHOST;

    let bind_addr = Ipv4Addr::UNSPECIFIED;
    let port = 8080;

    let addr = format!("{bind_addr}:{port}");

    let api = Api;

    let public_url = &format!("http://{public_addr}:{port}/openapi");

    let api_service =
        OpenApiService::new(api, "SDPlay", env!("CARGO_PKG_VERSION")).server(public_url);

    log::info!("Starting openapi service at {}", public_url);

    // TODO pass this around as state
    let (tx_stop, _rx_stop) = broadcast::channel::<()>(1);

    let openapi_explorer = api_service.swagger_ui();
    let oapi_spec_json = api_service.spec_endpoint();
    let oapi_spec_yaml = api_service.spec_endpoint_yaml();

    let app = Route::new()
        .nest("/openapi", api_service)
        .nest("/doc", openapi_explorer)
        .nest("/openapi/json", oapi_spec_json)
        .nest("/openapi/yaml", oapi_spec_yaml)
        .data(tx_stop);

    poem::Server::new(TcpListener::bind(addr)).run(app).await?;

    log::info!("Server stopped.");

    Ok(())
}
