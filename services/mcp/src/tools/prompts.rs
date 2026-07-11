use rmcp::model::{
    GetPromptRequestParams, GetPromptResult, Prompt, PromptArgument, PromptMessage,
    PromptMessageRole,
};
use rmcp::ErrorData;

pub(crate) fn or_none(s: &str) -> &str {
    if s.is_empty() {
        "(none)"
    } else {
        s
    }
}

fn prompt_arg(name: &str, description: &str, required: bool) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(required)
}

pub(crate) fn all_prompts() -> Vec<Prompt> {
    vec![
        Prompt::new(
            "vexa.meeting_prep",
            Some("Parse link, request bot, and attach meeting notes/metadata."),
            Some(vec![
                prompt_arg(
                    "meeting_url",
                    "Full meeting URL (recommended for Teams/Zoom).",
                    false,
                ),
                prompt_arg(
                    "meeting_platform",
                    "google_meet | teams | zoom (optional if meeting_url is provided).",
                    false,
                ),
                prompt_arg(
                    "meeting_id",
                    "Native meeting ID (optional if meeting_url is provided).",
                    false,
                ),
                prompt_arg(
                    "notes",
                    "Optional notes/agenda/context to store on the meeting.",
                    false,
                ),
            ]),
        )
        .with_title("Kioku: Meeting Prep"),
        Prompt::new(
            "vexa.during_meeting",
            Some("Check bot status and retrieve current transcript snapshot."),
            Some(vec![
                prompt_arg("meeting_platform", "google_meet | teams | zoom", true),
                prompt_arg("meeting_id", "Native meeting ID", true),
            ]),
        )
        .with_title("Kioku: During Meeting"),
        Prompt::new(
            "vexa.post_meeting",
            Some("Fetch bundle (notes, recordings, share link) and produce follow-ups."),
            Some(vec![
                prompt_arg("meeting_platform", "google_meet | teams | zoom", true),
                prompt_arg("meeting_id", "Native meeting ID", true),
            ]),
        )
        .with_title("Kioku: Post Meeting"),
        Prompt::new(
            "vexa.teams_link_help",
            Some("Supported Teams links and passcode requirements (issues #105/#110)."),
            Some(vec![prompt_arg(
                "meeting_url",
                "Teams meeting URL from the user",
                false,
            )]),
        )
        .with_title("Kioku: Teams Link Help"),
    ]
}

fn prompt_text(text: impl Into<String>) -> PromptMessage {
    PromptMessage::new_text(PromptMessageRole::User, text.into())
}

pub(crate) fn dispatch(request: GetPromptRequestParams) -> Result<GetPromptResult, ErrorData> {
    let args = request.arguments.unwrap_or_default();
    let get = |k: &str| {
        args.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };

    match request.name.as_str() {
        "vexa.meeting_prep" => {
            let (meeting_url, meeting_platform, meeting_id, notes) = (
                get("meeting_url"),
                get("meeting_platform"),
                get("meeting_id"),
                get("notes"),
            );
            Ok(GetPromptResult::new(vec![prompt_text(format!(
                    "You are helping me prepare a meeting using Kioku.\n\n\
                     Goals:\n1. Identify meeting platform + native meeting id (+ passcode if needed).\n\
                     2. Request the meeting bot (idempotent).\n\
                     3. Store meeting notes/metadata so it appears in transcript responses.\n\n\
                     Rules:\n- Prefer calling `parse_meeting_link` when `meeting_url` is provided.\n\
                     - For Teams: only `teams.live.com/meet/<id>?p=<passcode>` is supported; \
                     `teams.microsoft.com/l/meetup-join/...` is not supported.\n\
                     - When requesting a bot, pass `meeting_url` if you have it; otherwise use \
                     `native_meeting_id` (+ `passcode` for Teams, from ?p=).\n\
                     - After the meeting exists, call `update_meeting_data` with `notes` if provided.\n\n\
                     Input:\n- meeting_url: {}\n- meeting_platform: {}\n- meeting_id: {}\n- notes: {}\n\n\
                     Now do the tool calls and tell me what you did and what to do next.",
                    or_none(&meeting_url), or_none(&meeting_platform), or_none(&meeting_id), or_none(&notes)
                ))]).with_description("Meeting prep flow using Kioku MCP tools.".to_string()))
        }
        "vexa.during_meeting" => {
            let (meeting_platform, meeting_id) = (get("meeting_platform"), get("meeting_id"));
            Ok(GetPromptResult::new(vec![prompt_text(format!(
                    "You are my during-meeting assistant using Kioku.\n\n\
                     Meeting: platform={meeting_platform}, id={meeting_id}\n\n\
                     Steps:\n- Call `get_bot_status` to confirm the bot is active / requested.\n\
                     - Call `get_meeting_transcript` to fetch the current transcript snapshot.\n\
                     - If the transcript is empty, explain whether the meeting may not have started, \
                     bot may not be admitted yet, or transcription isn't producing segments.\n\n\
                     Then summarize key points and action items so far."
                ))]).with_description("During-meeting helper prompt using Kioku MCP tools.".to_string()))
        }
        "vexa.post_meeting" => {
            let (meeting_platform, meeting_id) = (get("meeting_platform"), get("meeting_id"));
            Ok(GetPromptResult::new(vec![prompt_text(format!(
                    "You are my post-meeting assistant using Kioku.\n\n\
                     Meeting: platform={meeting_platform}, id={meeting_id}\n\n\
                     Steps:\n- Call `get_meeting_bundle` (segments off) to fetch meeting status, notes, recordings, and share link.\n\
                     - If recordings exist, resolve download URLs if needed.\n\
                     - Produce:\n  1) concise summary\n  2) decisions\n\
                     3) action items with owners (if known) and due dates (if mentioned)\n  4) open questions\n"
                ))]).with_description("Post-meeting helper prompt using Kioku MCP tools.".to_string()))
        }
        "vexa.teams_link_help" => {
            let meeting_url = get("meeting_url");
            Ok(GetPromptResult::new(vec![prompt_text(format!(
                    "Help me troubleshoot a Microsoft Teams meeting link for Kioku.\n\n\
                     User link: {}\n\n\
                     Checklist:\n- If link is `teams.live.com/meet/<id>?p=<passcode>`:\n\
                     - native_meeting_id = <id> (10-15 digits)\n\
                     - passcode = value of ?p= (often required)\n\
                     - Prefer using `meeting_url` directly with `request_meeting_bot`.\n\
                     - If link is `teams.microsoft.com/l/meetup-join/...`: explain it's not supported yet (issues #105/#110).\n\
                     - If passcode fails validation, explain constraints (8-20 alphanumeric) and ask for a corrected link.\n\n\
                     If a link is provided, call `parse_meeting_link` and show the extracted fields.",
                    if meeting_url.is_empty() { "(none provided)".to_string() } else { meeting_url }
                ))]).with_description("Teams link troubleshooting prompt.".to_string()))
        }
        other => Err(ErrorData::invalid_params(
            format!("Unknown prompt: {other}"),
            None,
        )),
    }
}
