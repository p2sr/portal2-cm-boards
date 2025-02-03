use crate::{
    models::{
        changelog::{
            Changelog, ChangelogInsert, HistoryParams, ScoreLookup, ScoreParams,
            SubmissionChangelog,
        },
        chapters::OptIDs,
        sp::*,
        users::{Users, UsersPage},
    },
    tools::{
        cache::{read_from_file, write_to_file, CacheState, SP_PREVIEWS},
        config::Config,
        error::Result,
        helpers::{check_for_valid_score, score},
    },
};
use actix_web::{get, post, put, web, Responder};
use sqlx::PgPool;

// TODO: Invalidate cache when a time is banned/verified/when a player is banned.
/// **GET** method to handle the preview page showing all singleplayer maps.
///
/// Initial load tends to be relatively slow, but the information cached, and
/// remains in cache until a new singleplayer score is submitted
///
/// ## Example endpoints:
///  - **Default**           
///     - `/api/v1/sp`
///
/// Makes a call to the underlying [SpPreview::get_sp_previews]
/// **or** uses a cached value.
///
/// ## Example JSON output
///
/// ```json
/// [
///     {
///         "map_id": "47458",
///         "scores": [
///             {
///                 "profile_number": "76561198795823814",
///                 "score": 2326,
///                 "youtube_id": "DPgJgmLmzCw?start=0",
///                 "category_id": 1,
///                 "user_name": "Royal",
///                 "map_id": "47458"
///             },...]}]
/// ```
#[get("/sp")]
async fn sp(pool: web::Data<PgPool>, cache: web::Data<CacheState>) -> Result<impl Responder> {
    // See if we can utilize the cache
    if !cache.get_current_state(SP_PREVIEWS).await {
        let sp_previews = SpPreview::get_sp_previews(pool.get_ref()).await?;
        if write_to_file("sp_previews", &sp_previews).await.is_ok() {
            cache.update_current_state(SP_PREVIEWS, true).await;
        } else {
            eprintln!("Could not write cache for coop previews");
        }
        Ok(web::Json(sp_previews))
    } else {
        Ok(web::Json(
            read_from_file::<Vec<Vec<SpPreview>>>("sp_previews").await?,
        ))
    }
}

/// **GET** method to generate a single player map page [SpRanked] for a given map_id
///
/// ## Parameters:
/// - `cat_id`           
///     - **Optional** - `i32` - The ID of the category you want a Single Player Ranked Page for.
/// - `game_id`
///     - **Optional** - `i32` - The ID of the game you want a Single Player Ranked Page for. Defaults to the base game (1).
///
/// ## Example endpoint
/// - **Default**
///     - `/api/v1/map/sp/47802` - Will assume default category ID
/// - **Specific Category**                   
///     - `/api/v1/map/sp/47802?cat_id=88`
/// - **Specific Game**
///     - `/api/v1/map/sp/47802?game_id=1`
///
/// Makes a call to the underlying [SpMap::get_sp_map_page].
///
/// ## Example JSON output
///
/// ```json
/// [
///     {
///         "map_data": {
///             "timestamp": "2021-04-28T06:51:16",
///             "profile_number": "76561198254956991",
///             "score": 1729,
///             "demo_id": 21885,
///             "youtube_id": "MtwWXAO2E5c?start=0",
///             "submission": false,
///             "note": "https://www.youtube.com/watch?v=orwgEEaJln0",
///             "category_id": 88,
///             "user_name": "Zyntex",
///             "avatar": "https://steamcdn-a.akamaihd.net/steamcommunity/public/images/avatars/9d/9d160bcde456f7bb452b1ed9d9e740cd73f89266_full.jpg"
///         },
///         "rank": 1,
///         "points": 200.0
///     },....]
/// ```
#[get("/map/sp/{map_id}")]
pub async fn sp_map(
    map_id: web::Path<String>,
    ids: web::Query<OptIDs>,
    config: web::Data<Config>,
    cache: web::Data<CacheState>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let map_id = map_id.into_inner();
    let cat_id = ids
        .cat_id
        .unwrap_or(cache.into_inner().default_cat_ids[&map_id]);
    let sp_map = SpMap::get_sp_map_page(
        pool.get_ref(),
        &map_id,
        config.proof.results,
        cat_id,
        ids.game_id.unwrap_or(1),
    )
    .await?;
    let mut ranked_vec = Vec::with_capacity(config.proof.results as usize);
    for (i, entry) in sp_map.into_iter().enumerate() {
        // TODO: Fix tied ranks.
        ranked_vec.push(SpRanked {
            map_data: entry,
            rank: i as i32 + 1,
            points: score(i as i32 + 1),
        })
    }
    Ok(web::Json(ranked_vec))
}
/// **GET** method to return the profile number and score for all banned times on a given singleplayer map.
///
/// ## Example Endpoints
/// - **Default**
///     - `/api/v1/sp/all_banned/47458`
///
/// Makes a call to the underlying [SpBanned::get_sp_banned]
///
/// ## Example JSON output
///
/// ```json
/// [
///     {
///         "profile_number": "76561197961322276",
///         "score": -2147483648
///     },
///     {
///         "profile_number": "76561198096964328",
///         "score": -2147483648
///     }
/// ]
/// ```
#[get("/sp/all_banned/{map_id}")]
async fn sp_all_banned(map_id: web::Path<u64>, pool: web::Data<PgPool>) -> Result<impl Responder> {
    Ok(web::Json(
        SpBanned::get_sp_banned(pool.get_ref(), map_id.to_string()).await?,
    ))
}
/// **GET** method to return true or false given a `map_id`, `profile_number` and `score`
///
/// ## Parameters:
/// - `map_id`
///     - **Required** - `String` : Part of the endpoint, **not** a part of the query string.
/// - `profile_number`           
///     - **Required** `String` : ID for the player.
/// - `score`           
///     - **Required** `i32` : Time for the run.
/// - `cat_id`
///     - **Optional** `i32` : ID for the category, defaults to the map's default.
/// - `game_id`
///     - **Optional** - `i32` : ID for the game, defaults to base game, or 1.
///
/// ## Example Endpoints
/// - **With Parameters**
///     - `/api/v1/sp/banned/47458?profile_number=76561198823602829&score=2445`
/// - **With Optional**
///     - `/api/v1/sp/banned/47458?profile_number=76561198823602829&score=2445&cat_id=49&game_id=1`
///
/// Makes a call to the underlying [SpBanned::get_sp_banned]
///
/// ## Example JSON output
///
/// ```json
/// true
/// ```
#[get("/sp/banned/{map_id}")]
async fn sp_banned(
    map_id: web::Path<String>,
    params: web::Query<ScoreParams>,
    cache: web::Data<CacheState>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let map_id = map_id.into_inner();
    let cat_id = Some(
        params
            .cat_id
            .unwrap_or(cache.into_inner().default_cat_ids[&map_id]),
    );
    let is_banned = Changelog::check_banned_scores(
        pool.get_ref(),
        ScoreLookup {
            map_id,
            score: params.score,
            profile_number: params.profile_number.clone(),
            cat_id,
            game_id: Some(params.game_id.unwrap_or(1)),
        },
    )
    .await?;
    Ok(web::Json(is_banned))
}

/// **GET** method to return a history of scores on a current map, for a given player.
///
/// Query parameters represented as [HistoryParams]
///
/// ## Parameters:
/// - `profile_number`           
///     - **Required** - `String` : ID for the player.
/// - `map_id`           
///     - **Required** - `String` : ID for the map.
/// - `cat_id`           
///     - **Optional** `i32` : ID for the category. Defaults to the default category for the map.
/// - `game_id`
///     - **Optional**  `i32` : ID for the game. Defaults to the base game, or ID = 1.
///
/// ## Example Endpoints:
/// - **With Parameters**
///     - `/api/v1/sp/history?map_id=47458&profile_number=76561198795823814
/// - **With cat_id**
///     - `/api/v1/sp/history?map_id=47458&profile_number=76561198795823814&cat_id=49
///
/// Makes a call to the underlying [Users::get_user_data] & [Changelog::get_sp_pb_history]
///
/// # Example JSON output
///
/// - For a user that exists
///
/// ```json
/// {
///     "user_name": "Royal",
///     "avatar": "https://steamcdn-a.akamaihd.net/steamcommunity/public/images/avatars/d8/d84366b1be1f0439b0edc7fc8404fe2ea29a9c54_full.jpg",
///     "pb_history": [
///         {
///             "id": 152184,
///             "timestamp": "2021-07-06T09:11:04",
///             "profile_number": "76561198795823814",
///             "score": 2326,
///             "map_id": "47458",
///             "demo_id": 24527,
///             "banned": false,
///             "youtube_id": "DPgJgmLmzCw?start=0",
///             "previous_id": 141996,
///             "coop_id": null,
///             "post_rank": 1,
///             "pre_rank": 1,
///             "submission": true,
///             "note": "",
///             "category_id": 49,
///             "score_delta": -7,
///             "verified": true,
///             "admin_note": null
///         },..]}
/// ```
///
/// - For a user that does not exist.
///
/// ```json
/// {
///     "user_name": null,
///     "avatar": null,
///     "pb_history": null
/// }
/// ```
#[get("/sp/history")]
async fn sp_history(
    query: web::Query<HistoryParams>,
    pool: web::Data<PgPool>,
    cache: web::Data<CacheState>,
) -> Result<impl Responder> {
    let query = query.into_inner();
    let user_data: UsersPage;
    // Get information for the player (user_name and avatar).
    match Users::get_user_data(pool.get_ref(), &query.profile_number).await? {
        Some(res) => user_data = res,
        None => {
            return Ok(web::Json(SpPbHistory {
                user_name: None,
                avatar: None,
                pb_history: None,
            }))
        }
    }
    // Get Changelog data for all previous times.
    match Changelog::get_sp_pb_history(
        pool.get_ref(),
        &query.profile_number,
        &query.map_id,
        query
            .cat_id
            .unwrap_or(cache.into_inner().default_cat_ids[&query.map_id]),
        query.game_id.unwrap_or(1),
    )
    .await
    {
        Ok(changelog_data) => Ok(web::Json(SpPbHistory {
            user_name: Some(user_data.user_name),
            avatar: Some(user_data.avatar),
            pb_history: Some(changelog_data),
        })),
        Err(e) => {
            eprintln!("Could not find SP PB History -> {}", e);
            Ok(web::Json(SpPbHistory {
                user_name: None,
                avatar: None,
                pb_history: None,
            }))
        }
    }
}

// TODO: Potentially deprecate this function.
/// **GET** method for validating an SP Score. Mainly used by our backend that pulls times from the Steam leaderboards.
///
/// Query parameters represented as [ScoreLookup]
///
/// ## Parameters:
///    - `profile_number`           
///         - **Required**: `String`, ID for the player.
///    - `score`           
///         - **Required**: `i32`, Time for the run.
///    - `map_id`           
///         - **Required**: `String`, ID for the map.
///    - `cat_id`           
///         - **Optional**: `i32`, ID for the category. If left blank, will use the default for the map.
///
/// ## Example endpoints:
///  - **With Required**           
///     - `/api/v1/sp/validate?profile_number=76561198039230536&score=2346&map_id=47458`
///  - **With cat_id**   
///     - `/api/v1/sp/validate?profile_number=76561198039230536&score=2346&map_id=47458&?cat_id=1`
///
/// Makes a call to the underlying [check_for_valid_score]
///
/// ## Example JSON output where score is valid:
///
/// ```json
/// {
///     "previous_id": 102347,
///     "post_rank": 500,
///     "pre_rank": 3,
///     "score_delta": 2,
///     "banned": false
/// }
///
/// ## Example JSON output where score is **not** valid:
///
/// ```json
/// false
/// ```
#[get("/sp/validate")]
pub async fn sp_validate(
    pool: web::Data<PgPool>,
    data: web::Query<ScoreLookup>,
    cache: web::Data<CacheState>,
    config: web::Data<Config>,
) -> Result<impl Responder> {
    let details = check_for_valid_score(
        pool.get_ref(),
        &SubmissionChangelog {
            timestamp: "PLACEHOLDER".to_string(),
            profile_number: data.profile_number.clone(),
            score: data.score,
            map_id: data.map_id.clone(),
            category_id: Some(
                data.cat_id
                    .unwrap_or(cache.into_inner().default_cat_ids[&data.map_id]),
            ),
            game_id: Some(data.game_id.unwrap_or(1)),
            note: None,
            youtube_id: None,
        },
        config.proof.results,
    )
    .await?;
    Ok(web::Json(details))
}

// TODO: Deprecate this for changelog uploads.
/// Receives a new score to add to the DB.
#[post("/sp/post_score")]
async fn sp_post_score(
    params: web::Json<ChangelogInsert>,
    pool: web::Data<PgPool>,
    cache: web::Data<CacheState>,
) -> Result<impl Responder> {
    let id = Changelog::insert_changelog(pool.get_ref(), params.0).await?;
    cache.update_current_state(SP_PREVIEWS, false).await;
    Ok(web::Json(id))
}

// TODO: Make this more ergonomic? Don't require all values.
// TODO: Authentication should impact what a user can update.
// TODO: Update to return all.
/// **PUT** Method to update data for an existing singleplayer score.
///
/// Expects a JSON object as input. Best practice is to pass the current JSON [Changelog] object, and alter the fields you want changed.
///
/// ## Parameters:
/// - `id`
///     - **Required** : `i64` : The ID of the changelog entry you want to update.
/// - `timestamp`    
///     - **Required** : `String` : `%Y-%m-%d %H:%M:%S` (use `%20` to denote a space)
/// - `profile_number`
///     - **Required** : `String` : Steam ID Number
/// - `score`         
///     - **Required** : `i32` : Current board time format         
/// - `map_id`       
///     - **Required** : `String` : Steam ID for the map
/// - `banned`
///     - **Required** : `bool` : If the score is banned.
/// - `submission`
///     - **Required** : `bool` : If the score is a submission.
/// - `category_id`   
///     - `i32` : ID for the category being played.
/// - `demo_id`
///     - **Optional** : `i64` : ID for the associated demo.
/// - `youtube_id`
///     - **Optional** : `String`: Youtube URL Extension.
/// - `previous_id`
///     - **Optional** : `i64` : Previous score ID for the user.
/// - `coop_id`
///     - **Optional** : `i64` : Coop ID for the score.
/// - `post_rank`
///     - **Optional** : `i32` : Rank when submitted.
/// - `pre_rank`
///     - **Optional** : `i32` : Previous Rank when the new score was submitted.
/// - `note`          
///     - **Optional** : `String` : User comment for the run.
/// - `score_delta`
///     - **Optional** : `i32` : Difference in score between the two entries.
/// - `verified`
///     - **Optional** : `bool` : If the run is verified.
/// - `admin_note`
///     - **Optional** : `String` : Note by admin.
///
/// Makes a call to the underlying [Changelog::update_changelog]
///
/// ## Example JSON output
///
/// ```json
/// true
/// ```
#[put("/sp/update")]
async fn sp_update(
    params: web::Json<Changelog>,
    pool: web::Data<PgPool>,
    cache: web::Data<CacheState>,
) -> Result<impl Responder> {
    // TODO: Handle demo uploads.
    let changelog_entry = Changelog::update_changelog(pool.get_ref(), params.0).await?;
    cache.update_current_state(SP_PREVIEWS, false).await;
    Ok(web::Json(changelog_entry))
}
