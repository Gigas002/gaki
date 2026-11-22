//! Your own review of an app: read it and edit it over the CM
//! (`UserReviews.*`). Steam has no service method that creates a review;
//! [`crate::ops::store::write_review`] posts a new one through the store's
//! form instead.
//!
//! Generated from `proto/service_userreviews.proto` (see `build.rs`); the
//! `RpcMessage`/`RpcMethod` impls are hand-written like `wishlist.rs`.
use super::*;
use gaki_steam_vent::{EResult, NetworkError};
use gaki_steam_vent_proto::{RpcMessage, RpcMethod};
use std::io::{Read, Write};

include!(concat!(env!("OUT_DIR"), "/userreviews/mod.rs"));

use service_userreviews::{
    CUserReviews_GetIndividualRecommendations_Request,
    CUserReviews_GetIndividualRecommendations_Request_RecommendationRequest,
    CUserReviews_GetIndividualRecommendations_Response, CUserReviews_Update_Request,
    CUserReviews_Update_Response, RecommendationDetails,
};

macro_rules! rpc_message {
    ($t:ty) => {
        impl RpcMessage for $t {
            fn parse(reader: &mut dyn Read) -> ::protobuf::Result<Self> {
                <Self as ::protobuf::Message>::parse_from_reader(reader)
            }
            fn write(&self, writer: &mut dyn Write) -> ::protobuf::Result<()> {
                use ::protobuf::Message;
                self.write_to_writer(writer)
            }
            fn encode_size(&self) -> usize {
                use ::protobuf::Message;
                self.compute_size() as usize
            }
        }
    };
}

rpc_message!(CUserReviews_GetIndividualRecommendations_Request);
rpc_message!(CUserReviews_GetIndividualRecommendations_Response);
rpc_message!(CUserReviews_Update_Request);
rpc_message!(CUserReviews_Update_Response);

impl RpcMethod for CUserReviews_GetIndividualRecommendations_Request {
    const METHOD_NAME: &'static str = "UserReviews.GetIndividualRecommendations#1";
    type Response = CUserReviews_GetIndividualRecommendations_Response;
}

impl RpcMethod for CUserReviews_Update_Request {
    const METHOD_NAME: &'static str = "UserReviews.Update#1";
    type Response = CUserReviews_Update_Response;
}

/// A review to post: new, or replacing your current one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReviewDraft {
    pub text: String,
    /// Recommended.
    pub voted_up: bool,
    /// Shown on your profile and the store; private reviews are friends-only.
    pub is_public: bool,
    /// Steam's language name (`english`, `german`, …). `None` keeps an
    /// existing review's, or `english` for a new one.
    pub language: Option<String>,
    /// You got the game for free.
    pub received_compensation: bool,
    pub comments_disabled: bool,
}

impl ReviewDraft {
    /// Steam's review length limit, in characters.
    pub const MAX_CHARS: usize = 8000;

    /// The draft's problem, if Steam would refuse it.
    pub fn check(&self) -> Option<String> {
        let chars = self.text.trim().chars().count();
        if chars == 0 {
            Some("a review needs some text".into())
        } else if chars > Self::MAX_CHARS {
            Some(format!(
                "a review is at most {} characters; this one has {chars}",
                Self::MAX_CHARS
            ))
        } else {
            None
        }
    }
}

/// Your review of an app.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OwnReview {
    pub id: u64,
    pub app_id: u32,
    pub text: String,
    pub voted_up: bool,
    pub is_public: bool,
    pub language: String,
    pub received_compensation: bool,
    pub comments_disabled: bool,
    pub created: u64,
    pub updated: u64,
    pub votes_up: u32,
    pub votes_down: u32,
    pub votes_funny: u32,
    pub comments: u32,
}

impl OwnReview {
    fn from_details(r: &RecommendationDetails) -> Self {
        Self {
            id: r.recommendationid(),
            app_id: r.appid(),
            text: r.review().to_owned(),
            voted_up: r.voted_up(),
            is_public: r.is_public(),
            language: r.language().to_owned(),
            received_compensation: r.received_compensation(),
            comments_disabled: r.comments_disabled(),
            created: u64::from(r.time_created()),
            updated: u64::from(r.time_updated()),
            votes_up: r.votes_up(),
            votes_down: r.votes_down(),
            votes_funny: r.votes_funny(),
            comments: r.comment_count(),
        }
    }

    /// This review as a draft, to edit.
    pub fn draft(&self) -> ReviewDraft {
        ReviewDraft {
            text: self.text.clone(),
            voted_up: self.voted_up,
            is_public: self.is_public,
            language: Some(self.language.clone()).filter(|l| !l.is_empty()),
            received_compensation: self.received_compensation,
            comments_disabled: self.comments_disabled,
        }
    }
}

impl SteamClient {
    /// The signed-in account's review of `app_id`, if it wrote one.
    pub async fn own_review(&self, app_id: u32) -> Result<Option<OwnReview>> {
        let connection = self.require_connection()?;
        let mut wanted =
            CUserReviews_GetIndividualRecommendations_Request_RecommendationRequest::new();
        wanted.set_steamid(u64::from(connection.steam_id()));
        wanted.set_appid(app_id);
        let mut request = CUserReviews_GetIndividualRecommendations_Request::new();
        request.requests.push(wanted);
        // Steam answers `NoMatch` when there's no review (checked live:
        // an unknown method answers `Fail`).
        let response: CUserReviews_GetIndividualRecommendations_Response =
            match connection.service_method(request).await {
                Ok(response) => response,
                Err(NetworkError::ApiError(EResult::NoMatch)) => return Ok(None),
                Err(e) => {
                    return Err(anyhow::Error::new(e))
                        .context("failed calling UserReviews.GetIndividualRecommendations");
                }
            };
        Ok(response
            .recommendations
            .iter()
            .find(|r| r.recommendationid() != 0 && r.appid() == app_id)
            .map(OwnReview::from_details))
    }

    /// Replace review `id` with `draft`.
    pub async fn update_review(&self, id: u64, draft: &ReviewDraft) -> Result<()> {
        let connection = self.require_connection()?;
        let mut request = CUserReviews_Update_Request::new();
        request.set_recommendationid(id);
        request.set_review_text(draft.text.trim().to_owned());
        request.set_voted_up(draft.voted_up);
        request.set_is_public(draft.is_public);
        if let Some(language) = &draft.language {
            request.set_language(language.clone());
        }
        request.set_received_compensation(draft.received_compensation);
        request.set_comments_disabled(draft.comments_disabled);
        let _: CUserReviews_Update_Response = connection
            .service_method(request)
            .await
            .context("failed calling UserReviews.Update")?;
        Ok(())
    }
}
