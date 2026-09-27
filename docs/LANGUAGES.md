# Languages

claudeTalk has **one language setting**. Changing it switches, together:

| What | Where it comes from |
|---|---|
| Claude's voices (the gear's voice picker, the voices handed to parallel sessions) | the pack's `voices` |
| The dictation language (what Whisper transcribes) | the pack's `code`, unless **Detect language automatically** is on |
| Whisper's hint sentence (helps with technical words) | the pack's `whisper_prompt` |
| The default wake phrase ("Oye Claude", "Hey Claude", ...) | the pack's `wake_phrase`; a phrase you wrote yourself is kept |
| The fixed spoken lines: the gear's voice and volume samples, "I left the answer on screen" | the pack's `sample_voice`, `sample_volume`, `left_on_screen` |
| The gear panel and the tray menu | the pack's `ui` |
| What talk mode tells Claude | the rules name the language; Claude still answers in the language you write or speak to it |

**Spanish (`es`) and English (`en`) are built in.** Any other language works too:
Claude writes the pack the first time you ask for it.

A new install starts in Windows' display language when it is Spanish or
English, and in English otherwise. Installs from before v0.8 keep the
language their dictation was set to.

## Changing it

- Ask Claude in your words: "switch claudeTalk to English", "pon claudeTalk en español",
  "I want claudeTalk in French".
- `/talk set language en` (or `es`, `fr`, `pt`, a name like `english`, `francés`).
- Gear → **Dictation** → **Transcription** → **Language** (lists the built-in
  languages and every installed pack).

The dictation app applies it within a second; talking sessions switch to
voices of the new language.

**Detect language automatically** (same panel, or `/talk set dictation auto`)
lets Whisper detect what you speak on each dictation, for people who mix
languages. `/talk set dictation same` goes back to claudeTalk's language.

## Adding a language (what Claude does)

When there is no pack for a language, `claudetalk toggle set language fr`
exits with code 3 and prints the steps. The `talk` skill tells Claude to follow them:

1. **Pick voices.** `claudetalk voices fr` lists Microsoft Edge's voices for the
   language (`id  gender  locale`). Pick 2 to 6: alternate men and women, mix
   accents (fr-FR, fr-CA, fr-BE...), put the most natural one first, since it
   becomes the default voice.
2. **Translate the blank.** `claudetalk toggle template fr` prints the pack with
   English values. Translate every value: `name` (the language's own name),
   `wake_phrase` (2-3 words, "call + Claude"), the three spoken lines, the
   Whisper sentence (in the language, with dev words such as commit, hook,
   pull request), and every `ui` entry. Keep the JSON keys in English and the
   `{phrase}`, `{label}`, `{pct}` placeholders as they are. Keep panel labels
   short: the panel shrinks the text of a label that doesn't fit, but it
   reads better when it fits.
3. **Install it.** Save the file and run `claudetalk toggle pack FILE`. It is
   checked (real voice ids, no template placeholders left, placeholders kept)
   and saved to `%APPDATA%\claudeTalk\lang\fr.json`.
4. **Switch.** `claudetalk toggle set language fr`.

To fix a translation later, edit `%APPDATA%\claudeTalk\lang\<code>.json` (or
install a new pack over it). Built-in packs can't be replaced.

### Pack format

```json
{
  "code": "fr",
  "name": "Français",
  "voices": [
    { "id": "fr-FR-HenriNeural", "name": "Henri" },
    { "id": "fr-FR-DeniseNeural", "name": "Denise" }
  ],
  "wake_phrase": "Dis Claude",
  "sample_voice": "Bonjour, voici ma voix quand je te parle.",
  "sample_volume": "Bonjour, voici le volume de ma voix.",
  "left_on_screen": "Je t'ai laissé la réponse à l'écran.",
  "whisper_prompt": "Dictée en français pour Claude Code : commit, dépôt, hook, pull request, branche, terminal, script.",
  "ui": { "Dictation": "Dictée", "Language": "Langue", "Dictation: {label}": "Dictée : {label}" }
}
```

A missing `ui` entry stays in English. If a pack file breaks, claudeTalk falls
back to English for its own lines and voices.

## For developers

- Code: `native/ct-core/src/lang.rs` (packs, `apply`, `UI_TEXT`: every English
  text the dictation app shows). New UI text must be added to `UI_TEXT` and to
  the Spanish pack; a test fails if Spanish misses one.
- Settings keys in `%APPDATA%\claudeTalk\dictation.json`: `lang` (claudeTalk's
  language) and `language` (the dictation's: the same code, or `auto`).
- The gear panel translates at paint time (`ui::panel::tr`) and rebuilds its
  rows when `lang` changes.
