# Security notes (dx-tui)

## Secrets

- **Never** store API keys in the TUI session JSON.  
- Connected providers use `api_key_env` (env var name only) in `~/.config/dx/providers.toml`.  
- Channel tokens live in **dx-agent** config (`~/.config/dx/config.toml`) — treat that file as secret.  
- Share exports write transcripts to the system temp dir; delete after sending if sensitive.

## Channel gateway

- `/channels-start` spawns `dx-agent` gateway processes. Only run against trusted configs.  
- `/share-channel` and `/bind-channel` can expose conversation content to third-party messengers.  
- Review allowlists / pairing in dx-agent before enabling public channels.

## Project files injected into prompts

- `AGENTS.md` / `CLAUDE.md` are read from the project tree and sent to the model.  
- `.rtk/filters.json` only drops substrings (noise); it is not executed.  
- Do not put secrets in AGENTS.md.

## Local models (dx-flow)

- Local inference keeps data on-device when using dx-flow.  
- Remote Zen / OmniRoute / third-party providers send prompts off-machine.

## Clipboard

- Copy/share may place session content on the system clipboard.

## Reporting

Report vulnerabilities privately to the maintainers; do not open public issues with live tokens.
