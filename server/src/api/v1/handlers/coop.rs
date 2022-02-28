use crate::controllers::models::{
    Changelog, CoopBanned, CoopBundled, CoopBundledInsert, CoopMap, CoopPreviews, CoopRanked,
    Opti32, ScoreParams,
};
use crate::tools::cache::{read_from_file, write_to_file, CacheState};
use crate::tools::{config::Config, helpers::score};
use actix_web::{get, post, web, HttpResponse, Responder};
use sqlx::PgPool;
use std::collections::HashMap;

// TODO: Should use default cat_id
/// **GET** Returns top 7 information for each map, used to generate the previews page for Coop.
///
/// Example Endpoints:
/// - **Default**
///     - `/api/v1/coop`
#[get("/coop")]
async fn get_cooperative_preview(
    pool: web::Data<PgPool>,
    cache: web::Data<CacheState>,
) -> impl Responder {
    let state_data = &mut cache.current_state.lock().await;
    let is_cached = state_data.get_mut("coop_previews").unwrap();
    if !*is_cached {
        let res = CoopPreviews::get_coop_previews(pool.get_ref()).await;
        match res {
            Ok(previews) => {
                if write_to_file("coop_previews", &previews).await.is_ok() {
                    *is_cached = true;
                    HttpResponse::Ok().json(previews)
                } else {
                    eprintln!("Could not write cache for coop previews");
                    HttpResponse::Ok().json(previews)
                }
            }
            _ => HttpResponse::NotFound().body("Error fetching coop map previews."),
        }
    } else {
        let res = read_from_file::<Vec<CoopPreviews>>("coop_previews").await;
        match res {
            Ok(previews) => HttpResponse::Ok().json(previews),
            _ => HttpResponse::NotFound().body("Error fetching coop previews from cache"),
        }
    }
}
/// **GET** Returns all coop scores for a maps page on a specific category
///
/// Filtering of duplicate entries is handled.
///
/// **Required Parameters**: map_id
///
/// **Optional Parameters**: cat_id
///
/// Example Endpoints:
/// - **Default** - Will return the map page for the default category ID on the map_id specified.
///     - `/api/v1/map/coop/47802`
/// - **Specific Category ID** - Will use the cat_id specified.
///     - `/api/v1/map/coop/47802?cat_id=40`
#[get("/map/coop/{map_id}")]
async fn get_cooperative_maps(
    map_id: web::Path<u64>,
    cat_id: web::Query<Opti32>,
    config: web::Data<Config>,
    pool: web::Data<PgPool>,
) -> impl Responder {
    let res = CoopMap::get_coop_map_page(
        pool.get_ref(),
        map_id.to_string(),
        config.proof.results,
        cat_id.into_inner().cat_id,
    )
    .await;
    match res {
        Ok(coop_entries) => {
            let coop_entries_filtered =
                filter_coop_entries(coop_entries, config.proof.results as usize).await;
            HttpResponse::Ok().json(coop_entries_filtered)
        }
        _ => HttpResponse::NotFound().body("Error fetching Coop Map Page"),
    }
}
/// **GET** method to return all banned scores on a map for a specific category.
///
/// **Required Parameters**: map_id
///
/// **Optional Parameters**: cat_id
///
/// Example Endpoints:
/// - **Default** - Will return the banned times for the default category ID using the map_id supplied.
///     - `/api/v1/coop/map_banned/47802`
/// - **Specific Category ID** - Will use the cat_id specified.
///     - `/api/v1/coop/map_banned/47802?cat_id=40`
#[get("/coop/map_banned/{map_id}")]
async fn get_banned_scores_coop(
    map_id: web::Path<u64>,
    pool: web::Data<PgPool>,
    params: web::Query<Opti32>,
) -> impl Responder {
    let res = CoopBanned::get_coop_banned(
        pool.get_ref(),
        map_id.to_string(),
        params.into_inner().cat_id,
    )
    .await;
    match res {
        Ok(banned_entries) => HttpResponse::Ok().json(banned_entries),
        _ => HttpResponse::NotFound().body("Error fetching Coop banned information"),
    }
}
/// **GET** method to return a bool if a specific score is banned or not.
///
/// - `true`
///     - The time is banned.
/// - `false`
///     - The time is not banned.
///
/// Currently this uses the same logic for SP times.
///
/// **Required Parameters**: map_id, profile_number, score
///
/// **Optional Parameters**: cat_id
///
/// Example Endpoints:
/// - **Default** - Uses the map_id, profile_number & score provided, and assumes default category_id.
///     - `/api/v1/coop/time_banned/47825?profile_number=76561198823602829&score=1890`
/// - **Specific Category ID** - Will use the cat_id specified.
///     - `/api/v1/coop/time_banned/47825?profile_number=76561198823602829&score=1890&cat_id=62`
// TODO: Handle differently for coop?
#[get("/coop/time_banned/{map_id}")]
async fn post_banned_scores_coop(
    map_id: web::Path<u64>,
    params: web::Query<ScoreParams>,
    pool: web::Data<PgPool>,
) -> impl Responder {
    let res = Changelog::check_banned_scores(
        pool.get_ref(),
        map_id.to_string(),
        params.score,
        params.profile_number.clone(),
        params.cat_id,
    )
    .await;
    match res {
        Ok(banned_bool) => HttpResponse::Ok().json(banned_bool),
        Err(_) => HttpResponse::NotFound().body("Error checking ban information."),
    }
}

#[allow(unused_variables)]
#[post("/coop/post_score")]
async fn post_score_coop(
    params: web::Json<CoopBundledInsert>,
    pool: web::Data<PgPool>,
    cache: web::Data<CacheState>,
) -> impl Responder {
    let res = CoopBundled::insert_coop_bundled(pool.get_ref(), params.0).await;
    // match res {
    //     Ok(id) => {
    //     let state_data = &mut cache.current_state.lock().await;
    //     let is_cached = state_data.get_mut("coop_previews").unwrap();
    //     *is_cached = false;
    //     HttpResponse::Ok().json(id)
    // },
    //     _ => HttpResponse::NotFound().body("Error adding new score to database."),
    // }
    let id = 1;
    HttpResponse::Ok().json(id)
}

pub async fn filter_coop_entries(coop_entries: Vec<CoopMap>, limit: usize) -> Vec<CoopRanked> {
    //Filters out all obsolete times from the result, then truncates to x entries.
    let mut coop_entries_filtered = Vec::new();
    let mut remove_dups: HashMap<String, i32> = HashMap::with_capacity(limit);
    let mut i = 1;
    remove_dups.insert("".to_string(), 1);
    for entry in coop_entries {
        match remove_dups.insert(entry.profile_number1.clone(), 1) {
            // If player 1 has a better time, check to see if player 2 doesn't.
            Some(_) => match remove_dups.insert(entry.profile_number2.clone(), 1) {
                Some(_) => (),
                _ => {
                    coop_entries_filtered.push(CoopRanked {
                        map_data: entry.clone(),
                        rank: i,
                        points: score(i),
                    });
                    i += 1;
                }
            },
            // This case handles if player 1 doesn't have a better time, and it tries to add player 2 in as well, if two has a better time or not, this is included.
            _ => match remove_dups.insert(entry.profile_number2.clone(), 1) {
                Some(_) => {
                    coop_entries_filtered.push(CoopRanked {
                        map_data: entry.clone(),
                        rank: i,
                        points: score(i),
                    });
                    i += 1;
                }
                _ => {
                    coop_entries_filtered.push(CoopRanked {
                        map_data: entry.clone(),
                        rank: i,
                        points: score(i),
                    });
                    i += 1;
                }
            },
        }
    }
    coop_entries_filtered.truncate(limit);
    coop_entries_filtered
}
