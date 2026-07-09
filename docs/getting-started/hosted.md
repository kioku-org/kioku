---
title: "For Hosted Users"
description: "Capture meetings and search knowledge with the hosted Kioku service."
---

## Sign in

Go to [dashboard.kioku.chat](https://dashboard.kioku.chat) and sign in. The dashboard is
where you manage meetings, documents, workspace access, and API keys.

## Capture a meeting

1. Open **Join a Meeting**.
2. Choose Google Meet, Zoom, or Microsoft Teams.
3. Enter the meeting identifier. Enter the passcode for Teams when required.
4. Select **Join Meeting**.

The bot connects to the meeting and transcribes it while it runs. When the meeting finishes,
the transcript is available from the meeting view and can be searched with your other
workspace knowledge.

<Note>
  Leave **Authenticated** off unless you have configured a supported account connection. A
  non-authenticated bot may need to be admitted from the meeting waiting room.
</Note>

## Search your knowledge

Use **Search** to find relevant excerpts across your meeting transcripts, uploaded documents,
and ingested sessions. Results are scoped to your active workspace.

## Upload documents

Open **Documents** and upload a PDF, DOCX, PPTX, TXT, or Markdown file up to 50 MB. Kioku
extracts its text, indexes it, and makes it available to search alongside meetings.

## Connect an AI client

Connect Claude, Cursor, or another MCP client to:

```bash
https://mcp.kioku.chat/mcp
```

Authenticate with a Kioku token or API key. The [MCP client guide](/getting-started/mcp-cursor-claude)
has ready-to-paste configurations.

## Use the API

Create an API key from **Settings**. The hosted Hivemind API is available at
`https://api.kioku.chat`.

```bash
curl https://api.kioku.chat/meetings \
  -H "Authorization: Bearer YOUR_TOKEN"
```

See [Authentication](/api/authentication) for the supported credentials and
[API / CLI / MCP](/api-cli-mcp) for the endpoint reference.
