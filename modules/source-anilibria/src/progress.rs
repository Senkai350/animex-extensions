use std::cell::RefCell;
use std::collections::HashSet;

use anm_sdk::{config_get, http_request, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::exports::animex::module::{
    episode_hooks_api::{
        EpisodeIdentity, EpisodeProgress, HookEpisode, ReleaseIdentity, RemoteEpisodeProgress,
    },
    extension_settings_api::{SettingField, SettingKind, SettingOption},
};
use crate::{release_url, Component, API_BASE};

const SOURCE_ID: &str = "org.anilibria.source";
const LOGIN_PATH: &str = "/accounts/users/auth/login";
const TIMECODES_PATH: &str = "/accounts/users/me/views/timecodes";
const LOGIN_SETTING: &str = "login";
const PASSWORD_SETTING: &str = "password";
const MODE_SETTING: &str = "sync-mode";
const BOTH: &str = "both";
const IMPORT: &str = "import";
const EXPORT: &str = "export";
const OFF: &str = "off";

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

#[derive(PartialEq, Serialize)]
struct Credentials {
    login: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginResponse {
    token: String,
}

#[derive(Deserialize, Serialize)]
struct Timecode {
    release_episode_id: String,
    time: f64,
    is_watched: bool,
}

#[derive(Deserialize)]
struct EpisodeView {
    #[serde(flatten)]
    timecode: Timecode,
    updated_at: Option<String>,
}

#[derive(Serialize)]
struct EpisodeToClear<'a> {
    release_episode_id: &'a str,
}

struct Session {
    credentials: Credentials,
    token: String,
    last_write: Option<Timecode>,
}

impl crate::exports::animex::module::extension_settings_api::Guest for Component {
    fn settings() -> Vec<SettingField> {
        vec![
            SettingField {
                id: LOGIN_SETTING.into(),
                kind: SettingKind::Input,
                label: Some("Логин или почта AniLiberty".into()),
                description: None,
                options: Vec::new(),
                secret: false,
            },
            SettingField {
                id: PASSWORD_SETTING.into(),
                kind: SettingKind::Input,
                label: Some("Пароль AniLiberty".into()),
                description: Some("Используется для входа в AniLiberty.".into()),
                options: Vec::new(),
                secret: true,
            },
            SettingField {
                id: MODE_SETTING.into(),
                kind: SettingKind::Select,
                label: Some("Синхронизация прогресса".into()),
                description: Some("Действует для серий источника AniLibria.".into()),
                options: [
                    (BOTH, "В обе стороны"),
                    (IMPORT, "Получать из AniLiberty"),
                    (EXPORT, "Отправлять в AniLiberty"),
                    (OFF, "Выключена"),
                ]
                .into_iter()
                .map(|(value, label)| SettingOption {
                    value: value.into(),
                    label: label.into(),
                })
                .collect(),
                secret: false,
            },
        ]
    }
}

impl crate::exports::animex::module::episode_hooks_api::Guest for Component {
    fn progress_updated(episode: EpisodeIdentity, progress: EpisodeProgress) -> Result<(), String> {
        if !can_sync_episode(&episode) || !valid_position(progress.position_sec) {
            return Ok(());
        }
        with_session(EXPORT, |session| {
            session.save(Timecode {
                release_episode_id: episode.episode_id,
                time: progress.position_sec,
                is_watched: progress.watched,
            })
        })?;
        Ok(())
    }

    fn episode_watched(episode: EpisodeIdentity, watched: bool) -> Result<(), String> {
        if !can_sync_episode(&episode) {
            return Ok(());
        }
        with_session(EXPORT, |session| {
            if !watched {
                return session.clear(&episode.episode_id);
            }
            let time = match session.last_write.as_ref() {
                Some(last) if last.release_episode_id == episode.episode_id => last.time,
                _ => session
                    .release_timecodes(&episode.release.release_id)?
                    .into_iter()
                    .find(|view| view.timecode.release_episode_id == episode.episode_id)
                    .map(|view| view.timecode.time)
                    .filter(|time| valid_position(*time))
                    .unwrap_or_default(),
            };
            session.save(Timecode {
                release_episode_id: episode.episode_id,
                time,
                is_watched: true,
            })
        })?;
        Ok(())
    }

    fn episodes_loaded(
        release: ReleaseIdentity,
        episodes: Vec<HookEpisode>,
    ) -> Result<Vec<RemoteEpisodeProgress>, String> {
        if release.source_id != SOURCE_ID
            || !episodes.iter().any(|episode| is_uuid(&episode.episode_id))
        {
            return Ok(Vec::new());
        }
        with_session(IMPORT, |session| {
            let views = session.release_timecodes(&release.release_id)?;
            Ok(map_progress(&episodes, views))
        })
        .map(Option::unwrap_or_default)
    }
}

impl Session {
    fn request(
        &mut self,
        url: &str,
        method: &str,
        body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, String> {
        let response = send(url, method, Some(&self.token), body.clone())?;
        if matches!(response.status, 401 | 403) {
            self.token = login(&self.credentials)?;
            self.last_write = None;
            let response = send(url, method, Some(&self.token), body)?;
            return require_success(response);
        }
        require_success(response)
    }

    fn save(&mut self, timecode: Timecode) -> Result<(), String> {
        let body = encode(&[&timecode])?;
        self.request(&format!("{API_BASE}{TIMECODES_PATH}"), "POST", Some(body))?;
        self.last_write = Some(timecode);
        Ok(())
    }

    fn clear(&mut self, episode_id: &str) -> Result<(), String> {
        let body = encode(&[EpisodeToClear {
            release_episode_id: episode_id,
        }])?;
        self.request(&format!("{API_BASE}{TIMECODES_PATH}"), "DELETE", Some(body))?;
        if self
            .last_write
            .as_ref()
            .is_some_and(|last| last.release_episode_id == episode_id)
        {
            self.last_write = None;
        }
        Ok(())
    }

    fn release_timecodes(&mut self, release_id: &str) -> Result<Vec<EpisodeView>, String> {
        let url = format!("{}/episodes/timecodes", release_url(release_id)?);
        let response = self.request(&url, "GET", None)?;
        serde_json::from_slice(&response.body)
            .map_err(|_| "Не удалось прочитать прогресс AniLiberty".into())
    }
}

fn setting(id: &str) -> Result<Option<String>, String> {
    config_get(&format!("settings.{id}"))
}

fn with_session<T>(
    direction: &str,
    run: impl FnOnce(&mut Session) -> Result<T, String>,
) -> Result<Option<T>, String> {
    let mode = setting(MODE_SETTING)?.unwrap_or_else(|| BOTH.into());
    let login_name = setting(LOGIN_SETTING)?
        .unwrap_or_default()
        .trim()
        .to_owned();
    let password = setting(PASSWORD_SETTING)?.unwrap_or_default();

    SESSION.with(|state| {
        let mut session = state.borrow_mut();
        if mode == OFF || login_name.is_empty() || password.is_empty() {
            *session = None;
            return Ok(None);
        }
        let credentials = Credentials {
            login: login_name,
            password,
        };
        if session
            .as_ref()
            .is_some_and(|current| current.credentials != credentials)
        {
            *session = None;
        }
        if mode != BOTH && mode != direction {
            return Ok(None);
        }
        let current = match session.as_mut() {
            Some(current) => current,
            None => session.insert(Session {
                token: login(&credentials)?,
                credentials,
                last_write: None,
            }),
        };
        run(current).map(Some)
    })
}

fn login(credentials: &Credentials) -> Result<String, String> {
    let url = format!("{API_BASE}{LOGIN_PATH}");
    let response = send(&url, "POST", None, Some(encode(credentials)?))?;
    if matches!(response.status, 401 | 403 | 422) {
        return Err(
            "Не удалось войти в AniLiberty. Проверьте логин и пароль в настройках расширения."
                .into(),
        );
    }
    let response = require_success(response)?;
    let token = serde_json::from_slice::<LoginResponse>(&response.body)
        .map_err(|_| "Не удалось прочитать ответ входа AniLiberty".to_string())?
        .token;
    if token.is_empty() || !token.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("AniLiberty вернул некорректный токен".into());
    }
    Ok(token)
}

fn send(
    url: &str,
    method: &str,
    token: Option<&str>,
    body: Option<Vec<u8>>,
) -> Result<HttpResponse, String> {
    let mut headers = vec![("accept".into(), "application/json".into())];
    if body.is_some() {
        headers.push(("content-type".into(), "application/json".into()));
    }
    if let Some(token) = token {
        headers.push(("authorization".into(), format!("Bearer {token}")));
    }
    http_request(&HttpRequest {
        url: url.into(),
        method: Some(method.into()),
        headers,
        body,
    })
    .map_err(|_| "Не удалось связаться с AniLiberty".into())
}

fn encode(body: &impl Serialize) -> Result<Vec<u8>, String> {
    serde_json::to_vec(body).map_err(|_| "Не удалось подготовить запрос AniLiberty".into())
}

fn require_success(response: HttpResponse) -> Result<HttpResponse, String> {
    if (200..300).contains(&response.status) {
        Ok(response)
    } else {
        Err(format!("AniLiberty вернул HTTP {}", response.status))
    }
}

fn can_sync_episode(episode: &EpisodeIdentity) -> bool {
    episode.release.source_id == SOURCE_ID && is_uuid(&episode.episode_id)
}

fn is_uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn valid_position(position: f64) -> bool {
    position.is_finite() && position >= 0.0
}

fn map_progress(episodes: &[HookEpisode], views: Vec<EpisodeView>) -> Vec<RemoteEpisodeProgress> {
    let ids: HashSet<_> = episodes
        .iter()
        .map(|episode| episode.episode_id.as_str())
        .collect();
    views
        .into_iter()
        .filter_map(|view| {
            let timecode = view.timecode;
            if !ids.contains(timecode.release_episode_id.as_str()) {
                return None;
            }
            Some(RemoteEpisodeProgress {
                episode_id: timecode.release_episode_id,
                position_sec: valid_position(timecode.time).then_some(timecode.time),
                duration_sec: None,
                watched: Some(timecode.is_watched),
                updated_at: view.updated_at,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_timecodes_for_requested_episode_ids_and_keeps_server_time() {
        let uuid = "9b5e26ee-598f-4b8b-b77e-188d3e456318";
        let payload = format!(
            r#"[
                {{"release_episode_id":"{uuid}","time":127.45,"is_watched":true,"updated_at":"2025-06-23T14:00:00+00:00"}},
                {{"release_episode_id":"9c5f39e1-56dd-4d44-950a-e1a4201426a4","time":2,"is_watched":false}}
            ]"#
        );
        let views = serde_json::from_str(&payload).unwrap();
        let progress = map_progress(
            &[HookEpisode {
                episode_id: uuid.into(),
                number: Some(1.0),
            }],
            views,
        );
        assert_eq!(progress.len(), 1);
        assert_eq!(progress[0].episode_id, uuid);
        assert_eq!(progress[0].position_sec, Some(127.45));
        assert_eq!(progress[0].watched, Some(true));
        assert_eq!(
            progress[0].updated_at.as_deref(),
            Some("2025-06-23T14:00:00+00:00")
        );
    }

    #[test]
    fn legacy_synthesized_episode_ids_are_not_sent_to_aniliberty() {
        assert!(!is_uuid("9839-1"));
        assert!(!is_uuid("9b5e26ee-598f-4b8b-b77e_188d3e456318"));
        assert!(is_uuid("9b5e26ee-598f-4b8b-b77e-188d3e456318"));
    }
}
