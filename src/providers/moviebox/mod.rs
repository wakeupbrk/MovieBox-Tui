pub mod client;
pub mod crypto;

use client::{MovieBoxClient, ScraperError};
use serde_json::{Value, json};

impl MovieBoxClient {
    pub async fn search(&self, query: &str, page: usize) -> Result<Value, ScraperError> {
        let payload = json!({
            "keyword": query,
            "page": page,
            "perPage": 20,
            "subjectType": "All",
            "tabId": "All"
        });
        self.post("/wefeed-mobile-bff/subject-api/search/v2", &payload)
            .await
    }

    pub async fn suggest(&self, query: &str) -> Result<Value, ScraperError> {
        let payload = json!({
            "keyword": query,
            "page": 1,
            "perPage": 20,
            "subjectType": "All",
            "tabId": "All"
        });
        self.post("/wefeed-mobile-bff/subject-api/search/v2", &payload)
            .await
    }

    pub async fn get_details(&self, subject_id: &str) -> Result<Value, ScraperError> {
        let path = format!(
            "/wefeed-mobile-bff/subject-api/get?subjectId={}",
            subject_id
        );
        let mut details = self.get(&path).await?;

        let stype = details
            .get("subjectType")
            .and_then(|s| s.as_i64())
            .or_else(|| details.get("stype").and_then(|s| s.as_i64()))
            .unwrap_or(1);

        if stype == 2 {
            let season_path = format!(
                "/wefeed-mobile-bff/subject-api/season-info?subjectId={}",
                subject_id
            );
            if let Ok(season_info) = self.get(&season_path).await {
                if let Value::Object(ref mut map) = details {
                    map.insert("seasons".to_string(), season_info);
                }
            }
        }

        Ok(details)
    }

    pub async fn get_homepage(&self, tab_id: &str, page: usize) -> Result<Value, ScraperError> {
        let path = format!(
            "/wefeed-mobile-bff/tab-operating?page={}&tabId={}&version=",
            page, tab_id
        );
        self.get(&path).await
    }

    pub async fn get_resources(
        &self,
        subject_id: &str,
        season: usize,
        episode: usize,
        page: usize,
        resolution: Option<&str>,
        per_page: usize,
    ) -> Result<Value, ScraperError> {
        let res_param = if let Some(r) = resolution {
            if r.is_empty() {
                String::new()
            } else {
                format!("&resolution={}", r)
            }
        } else {
            String::new()
        };

        let path = if season == 0 && episode == 0 {
            format!(
                "/wefeed-mobile-bff/subject-api/resource?subjectId={}&page={}&perPage={}{}",
                subject_id, page, per_page, res_param
            )
        } else {
            format!(
                "/wefeed-mobile-bff/subject-api/resource?subjectId={}&se={}&ep={}&page={}&perPage={}{}",
                subject_id, season, episode, page, per_page, res_param
            )
        };
        self.get(&path).await
    }

    pub async fn fetch_resource_page(
        &self,
        subject_id: &str,
        resolution: u32,
        page: usize,
    ) -> Result<(Vec<Value>, Value), ScraperError> {
        let res_param = if resolution == 0 {
            String::new()
        } else {
            format!("&resolution={}", resolution)
        };

        let path = format!(
            "/wefeed-mobile-bff/subject-api/resource?subjectId={}&page={}&perPage=20{}",
            subject_id, page, res_param
        );

        let res = self.get(&path).await?;

        let items = res
            .get("list")
            .and_then(|l| l.as_array())
            .cloned()
            .unwrap_or_default();

        let pager = res.get("pager").cloned().unwrap_or_else(|| json!({}));

        Ok((items, pager))
    }

    pub async fn fetch_collection_resolutions(
        &self,
        subject_id: &str,
    ) -> Result<Vec<u32>, ScraperError> {
        let path = format!(
            "/wefeed-mobile-bff/subject-api/resource?subjectId={}&page=1&perPage=20",
            subject_id
        );
        let res = self.get(&path).await?;

        let mut resolutions = Vec::new();
        if let Some(cols) = res.get("collectionResolutions").and_then(|c| c.as_array()) {
            for col in cols {
                if let Some(r) = col.get("resolution").and_then(|v| v.as_u64()) {
                    if r > 0 {
                        resolutions.push(r as u32);
                    }
                }
            }
        }

        resolutions.sort_by(|a, b| b.cmp(a));
        resolutions.dedup();

        // Prefer real catalog resolutions. Do NOT invent 1080/720/… — that creates
        // empty pages and false "no streams" failures. Unfiltered (0) is always tried
        // by fetch_streams_for_episode.
        Ok(resolutions)
    }

    /// Fetch every playable resource for a movie (se=0,ep=0) or series episode.
    ///
    /// Strategy:
    /// 1. Unfiltered catalog scan (most complete) — jump to `estimated_page` then fall
    ///    back to page 1 if that miss (old bug: empty estimate page aborted the search).
    /// 2. Per-resolution scans to pick up extra qualities once we know the episode exists.
    /// 3. One identity rotate + retry if the first pass returns nothing.
    pub async fn fetch_streams_for_episode(
        &self,
        subject_id: &str,
        season: usize,
        episode: usize,
        estimated_page: usize,
        known_resolutions: &[u32],
    ) -> Result<Vec<Value>, ScraperError> {
        let is_movie = season == 0 && episode == 0;
        let mut last_err: Option<ScraperError> = None;

        for attempt in 0..2 {
            if attempt == 1 {
                self.refresh_identity();
                let _ = self.init().await;
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }

            match self
                .fetch_streams_once(
                    subject_id,
                    season,
                    episode,
                    is_movie,
                    estimated_page,
                    known_resolutions,
                )
                .await
            {
                Ok(items) if !items.is_empty() => return Ok(items),
                Ok(_) => {
                    // empty — retry with fresh identity
                }
                Err(e) => last_err = Some(e),
            }
        }

        if let Some(err) = last_err {
            return Err(err);
        }
        Ok(Vec::new())
    }

    async fn fetch_streams_once(
        &self,
        subject_id: &str,
        season: usize,
        episode: usize,
        is_movie: bool,
        estimated_page: usize,
        known_resolutions: &[u32],
    ) -> Result<Vec<Value>, ScraperError> {
        let mut matched: Vec<Value> = Vec::new();
        let mut seen_links = std::collections::HashSet::<String>::new();

        let push_matches = |items: Vec<Value>,
                            matched: &mut Vec<Value>,
                            seen: &mut std::collections::HashSet<String>,
                            season: usize,
                            episode: usize,
                            is_movie: bool| {
            for item in items {
                if !item_has_playable_link(&item) {
                    continue;
                }
                if is_movie {
                    // Movies are se=0/ep=0 (or missing). Accept any playable row.
                    if let Some(link) = item_link(&item) {
                        if seen.insert(link) {
                            matched.push(item);
                        }
                    }
                    continue;
                }
                if item_matches_episode(&item, season, episode) {
                    if let Some(link) = item_link(&item) {
                        if seen.insert(link) {
                            matched.push(item);
                        }
                    }
                }
            }
        };

        // --- Phase 1: unfiltered catalog (resolution=0) ---
        if is_movie {
            let mut page = 1usize;
            loop {
                match self.fetch_resource_page(subject_id, 0, page).await {
                    Ok((items, pager)) => {
                        let has_more = pager_has_more(&pager);
                        push_matches(
                            items,
                            &mut matched,
                            &mut seen_links,
                            season,
                            episode,
                            is_movie,
                        );
                        if !matched.is_empty() || !has_more || page >= 5 {
                            break;
                        }
                        page += 1;
                    }
                    Err(e) => {
                        if matched.is_empty() {
                            return Err(e);
                        }
                        break;
                    }
                }
            }
        } else {
            // Fast path: jump to estimated page for late seasons, then scan from page 1.
            // Critical fix: an empty estimated page must NOT abort the search.
            let estimate = estimated_page.max(1);
            let mut page_order: Vec<usize> = Vec::with_capacity(60);
            if estimate > 1 {
                page_order.push(estimate);
            }
            for p in 1..=60 {
                if !page_order.contains(&p) {
                    page_order.push(p);
                }
            }

            let mut sequential_end = false;
            for page in page_order {
                if sequential_end && page > 1 {
                    // We've already walked page 1..N until hasMore=false.
                    continue;
                }
                match self.fetch_resource_page(subject_id, 0, page).await {
                    Ok((items, pager)) => {
                        let has_more = pager_has_more(&pager);
                        let before = matched.len();
                        push_matches(
                            items,
                            &mut matched,
                            &mut seen_links,
                            season,
                            episode,
                            is_movie,
                        );
                        if matched.len() > before {
                            break;
                        }
                        // Finished the natural catalog while scanning forward from 1.
                        if page >= 1 && !has_more && page != estimate {
                            sequential_end = true;
                            if page >= estimate || estimate == 1 {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        if matched.is_empty() && page == 1 {
                            return Err(e);
                        }
                    }
                }
            }
        }

        // --- Phase 2: pull extra qualities for the same episode ---
        let mut resolutions: Vec<u32> = known_resolutions
            .iter()
            .copied()
            .filter(|r| *r > 0)
            .collect();
        if resolutions.is_empty() {
            // Discover from unfiltered page 1 if pool didn't know yet.
            if let Ok(discovered) = self.fetch_collection_resolutions(subject_id).await {
                resolutions = discovered;
            }
        }
        // Cap concurrent quality passes — we already have playable links if phase 1 hit.
        for &res in &resolutions {
            if res == 0 {
                continue;
            }
            // Only a few pages per resolution; jump to estimate first.
            let pages = if is_movie {
                vec![1usize]
            } else {
                let est = estimated_page.max(1);
                let mut ps = vec![est];
                if est != 1 {
                    ps.push(1);
                }
                if est > 2 {
                    ps.push(est.saturating_sub(1));
                }
                ps.push(est + 1);
                ps
            };
            for page in pages {
                match self.fetch_resource_page(subject_id, res, page).await {
                    Ok((items, _)) => {
                        push_matches(
                            items,
                            &mut matched,
                            &mut seen_links,
                            season,
                            episode,
                            is_movie,
                        );
                    }
                    Err(_) => continue,
                }
            }
        }

        // Prefer higher resolution first.
        matched.sort_by(|a, b| {
            let ra = a.get("resolution").and_then(|r| r.as_i64()).unwrap_or(0);
            let rb = b.get("resolution").and_then(|r| r.as_i64()).unwrap_or(0);
            rb.cmp(&ra)
        });

        Ok(matched)
    }

    pub async fn get_ext_captions(
        &self,
        subject_id: &str,
        resource_id: &str,
    ) -> Result<Value, ScraperError> {
        let path = format!(
            "/wefeed-mobile-bff/subject-api/get-ext-captions?subjectId={}&resourceId={}",
            subject_id, resource_id
        );
        self.get(&path).await
    }
}

fn item_link(item: &Value) -> Option<String> {
    item.get("resourceLink")
        .and_then(|l| l.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn item_has_playable_link(item: &Value) -> bool {
    item_link(item).is_some()
}

fn json_usize(value: Option<&Value>) -> Option<usize> {
    value.and_then(|v| {
        v.as_u64()
            .map(|n| n as usize)
            .or_else(|| v.as_i64().map(|n| n.max(0) as usize))
            .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
    })
}

fn item_matches_episode(item: &Value, season: usize, episode: usize) -> bool {
    let se = json_usize(item.get("se")).unwrap_or(0);
    let ep = json_usize(item.get("ep")).unwrap_or(0);
    se == season && ep == episode
}

fn pager_has_more(pager: &Value) -> bool {
    pager
        .get("hasMore")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}
