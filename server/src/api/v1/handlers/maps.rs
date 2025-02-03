use crate::{
    models::{
        chapters::GameID,
        maps::{IsCoop, Maps},
    },
    tools::error::Result,
};
use actix_web::{get, web, Responder};
use sqlx::PgPool;

/// **GET** method to return all map information for a given game.
///
/// ## Parameters:
/// - `game_id`
///     - **Optional** - `i32` : ID for the game that the map/chapter belongs to.
///                              If left empty, defaults to base-game (`id` = 1)
///
/// ## Example endpoints:
///  - **Default**
///     - `/api/v1/maps`
///  - **With game_id**
///     - `/api/v1/maps?game_id=1`
///
/// Makes a call to the underlying [Maps::get_maps]
///
/// ## Example JSON output
///
/// ``` json
/// [
///     {
///         "id": 51,
///         "steam_id": "47458",
///         "lp_id": "47459",
///         "name": "Portal Gun",
///         "chapter_id": 7,
///         "default_cat_id": 1,
///         "is_public": true
///     },...]
/// ```
#[get("/maps")]
async fn maps(pool: web::Data<PgPool>, query: web::Query<GameID>) -> Result<impl Responder> {
    Ok(web::Json(
        Maps::get_maps(pool.get_ref(), query.into_inner().game_id.unwrap_or(1)).await?,
    ))
}

/// **GET** method to return the default category ID for a given map
///
/// ## Example endpoints:
///  - **Default**
///     - `/api/v1/default_category/47458`
///
/// Makes a call to the underlying [Maps::get_maps]
///
/// ## Example JSON output
///
/// ```json
/// 49
/// ```
#[get("/default_category/{map}")]
async fn default_category(
    params: web::Path<u64>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    Ok(web::Json(
        Maps::get_default_cat(pool.get_ref(), params.to_string()).await?,
    ))
}

// TODO: Have this take an option<bool>? Somewhat more ergonomic in some places.
/// **GET** method to return the all steam_ids for a given game. Filters by if the map is coop or not.
///
/// ## Example endpoints:
///  - **With Parameters**
///     - `/api/v1/map_ids?is_coop=true`
///  - **Specific Game**
///     - `/api/v1/map_ids?is_coop=true&game_id=1`
///
/// Makes a call to the underlying [Maps::get_steam_ids]
///
/// ## Example JSON output
///
/// ```json
// [
//     "47741",
//     "47825",
//     "47828",
//     "47829",...]
/// ```
#[get("/map_ids")]
async fn map_ids(pool: web::Data<PgPool>, query: web::Query<IsCoop>) -> Result<impl Responder> {
    Ok(web::Json(
        Maps::get_steam_ids(pool.get_ref(), query.into_inner().is_coop).await?,
    ))
}
