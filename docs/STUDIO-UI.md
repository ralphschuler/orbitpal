# Studio UI acceptance checks

The Studio uses one save/status area, a persistent selected-bot context and a shared Skills & MCP library. UI language never changes a bot’s persona, conversation or voice language.

Use disposable profiles and fixture content for these regression checks. Never use real credentials, recordings or provider accounts. Browser fixtures verify UI behavior only; they do not establish native permission, updater, microphone or provider acceptance.

## Layout and keyboard

Review all eight navigation sections in German, English, French and Spanish at 1440 × 1000, 1024 × 800 and 760 × 900. Include several bots, long names and populated chat/library/history content. No page should scroll horizontally; the crew and narrow-window navigation may scroll within their own strips. Form controls and icon buttons need accessible names, visible focus and readable text. Optional setup details must not hide errors, transmission notices, grants or pending actions.

- Tab from the skip link into the main content. Navigate sections with Alt/Option+1–8 outside text fields. Use Ctrl/Cmd+K to reach the composer and Ctrl/Cmd+S to save the current context.
- Open the bot picker and press Escape. It must dismiss the picker without stopping output. Focus a help icon, then press Escape: dismiss only its tooltip.
- With a dirty library editor open, press F1 then Escape. Close only the keyboard help; preserve the editor and do not open a discard confirmation.
- During IME composition, Escape must not close a draft or trigger a global stop.
- Open a confirmation. Cancel receives initial focus; Tab and Shift+Tab wrap within the dialog. Background shortcuts must not save, send or navigate. Escape cancels only when no action is pending.
- Close the update dialog with Escape. Checking for updates must remain separate from installation confirmation.

## Draft and deletion regressions

| Setup / action | Expected result |
| --- | --- |
| Open a library editor without changing it; navigate away | No discard prompt. |
| Change a library field; choose another section or library item/tab | Ask before discarding. Cancel preserves all draft fields; confirm proceeds. |
| Keep a dirty shared-library draft and select another bot | Preserve the shared draft; display grants for the newly selected bot. |
| Save a library draft using its form or Ctrl/Cmd+S | On success clear the unsaved indicator; on failure retain the draft. |
| Start a pending library operation; attempt navigation or another global save | Stay in the library and prevent a duplicate save. Restore normal controls after completion or failure. |
| Open a discard confirmation; press Ctrl/Cmd+S | Do not save behind the confirmation. |
| Cancel knowledge-file removal or history clearing | Perform no destructive command. |
| Confirm removal/clearing with several bots present | Affect only the selected bot’s intended file/history. Other bots remain unchanged. |
| Make a confirmed operation fail | Keep the confirmation available and show the error; do not report success. |

Native desktop and OBS output remain separate renderers. Check that their backgrounds stay transparent and that changing Studio pages does not stop an explicitly started wake-word listener. Use synthetic playback for speech/emote timing checks; never start a real microphone automatically.
