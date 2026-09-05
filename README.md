# Regurgitate

A small, private notebook that helps your AI coding assistant avoid repeating mistakes.

When your assistant finds a useful solution, it can save a short lesson. On a
similar task later, it can check that lesson before trying the same wrong turn
again—even in a new chat or with another connected assistant.

The goal is less repeated work, fewer explanations from you, and less time
spent watching an assistant rediscover something it already figured out.
Regurgitate is still early; those benefits are the goal, not a guarantee.

## Install

Works with **Codex** and **Claude Code** on **Mac** or **64-bit Intel/AMD Linux**.
Your coding assistant must already be installed. Windows is not supported yet.

Choose your assistant below. If you use Agent of Empires, choose that option
instead. You do not need to download the source code or move files yourself.

To find **Terminal** on a Mac, press **Command + Space**, type `Terminal`, and
press **Enter**. On Linux, search your applications menu for `Terminal`.

<details>
<summary><strong>I use Codex</strong></summary>

1. Open the **Terminal** app on your computer.
2. Copy the entire block below, paste it into Terminal, and press **Enter**:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/vomitselfie/Regurgitate/releases/latest/download/install.sh |
  sh -s -- --agent codex
```

3. Wait for the messages beginning with `Installed` and `Connected`.
4. Close and reopen Codex so it can load Regurgitate.

That's the setup. You can keep working normally.

</details>

<details>
<summary><strong>I use Claude Code</strong></summary>

1. Open the **Terminal** app on your computer.
2. Copy the entire block below, paste it into Terminal, and press **Enter**:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/vomitselfie/Regurgitate/releases/latest/download/install.sh |
  sh -s -- --agent claude
```

3. Wait for the messages beginning with `Installed` and `Connected`.
4. Close and reopen Claude Code so it can load Regurgitate.

That's the setup. You can keep working normally.

</details>

<details>
<summary><strong>I use Agent of Empires (AoE)</strong></summary>

These steps are for an existing AoE installation, version 1.14 or newer within
version 1.x.

1. Open **Terminal**, paste this command, and press **Enter**:

```bash
aoe plugin install gh:vomitselfie/Regurgitate
```

2. Open AoE. If it is not running, run `aoe serve` in Terminal.
3. Open Regurgitate's plugin settings and click **Set up Codex** or **Set up Claude Code**.
4. Restart the assistant you connected.

The setup button matters: installing the plugin downloads it; setup connects
it to your assistant. You can also find the setup actions in AoE's command
palette by searching for `Regurgitate`.

If AoE says the plugin is already installed, use the update instructions below.

</details>

## What happens next?

Keep using your assistant as usual. It can save an occasional useful lesson and
check for relevant lessons when they could help. You do not need to write
lessons, keep a daily log, or mention Regurgitate in every chat.

It starts empty. No lessons yet—or nothing to recall for a particular task—is
normal. A useful result is an assistant avoiding a repeated mistake, not a
notification after every action.

Connected assistants can share reusable lessons on your computer. Lessons that
only apply to one project stay with that project.

## Is it private?

Regurgitate keeps its notebook **encrypted on your computer**. It does not
archive your conversations, source code, or terminal output. It saves short
lessons about approaches that worked or failed, plus limited information about
tool activity. There is no Regurgitate cloud service.

Your AI provider's normal data handling still applies when your assistant reads
a lesson. Regurgitate does not change your provider's privacy settings.

## Updates and help

<details>
<summary>How do I update it?</summary>

**Installed using the Codex or Claude Code command above?** Run that same command
again, then restart your assistant.

**Installed through AoE?** Run:

```bash
aoe plugin update vomitselfie.regurgitate
```

Then use **Set up Codex** or **Set up Claude Code** again and restart that
assistant. Use the same installation method you originally chose for each
assistant.

</details>

<details>
<summary>It asks for permission. What should I expect?</summary>

AoE setup asks to run the plugin and read or write the assistant's setup files.
The first memory request may also ask to access your computer's password/key
storage, which protects the notebook's encryption key.

Approve a request specifically for Regurgitate if you want it to proceed.
If the request is unclear, ask your assistant to explain it before approving.
Do not grant unrestricted terminal access just to make memory work.

</details>

<details>
<summary>I see an error, “command not found,” or “no matches”</summary>

- **“No matches” or no lessons:** normal when the notebook is new or has nothing relevant.
- **A message saying to add a folder to PATH:** this is only needed to run `regurgitate` yourself in Terminal. The connected assistant already has its location.
- **“Command not found” while installing:** note which command is missing and ask your assistant to help install that prerequisite.
- **Setup reports a conflict:** it found existing files it cannot safely replace. Ask your assistant to review them before replacing anything.
- **Memory is unavailable:** your computer's key storage or the assistant's permissions may need attention. Your main work can continue.

For help, [open an issue](https://github.com/vomitselfie/Regurgitate/issues) with
your operating system, assistant, and the error message. Remove private details
before sharing it; do not upload the notebook or encryption keys.

</details>

<details>
<summary>Technical details, manual installation, and commands</summary>

The installer checks the downloaded release, puts the program in `~/.local/bin`,
and connects your selected assistant. It does not need `sudo` or changes to your
shell profile. Existing settings are preserved; known untouched older skills
upgrade automatically.

Other agents that can run terminal commands can use Regurgitate too, but only
Codex and Claude Code currently have guided setup. Sharing requires the same OS
account, data location, and credential store.

- [Technical reference and advanced installation](docs/usage.md)
- [Architecture and privacy details](docs/architecture.md)
- [How lesson briefs work](docs/decision-briefs.md)
- [Roadmap](docs/roadmap.md)
- [Contributing and releases](docs/releasing.md)
- [Agent skill instructions](skills/regurgitate-recall/SKILL.md)

</details>

MIT licensed.
