// Built-in help in English (see ui/js/ayuda.js). Keep it in step with docs/manual.en.md.
//
// REQUIRED FORMAT (app/tests/i18n_ui.rs reads it with a line-based parser, without Node):
//   `  view: {`, `    intro: '…',`, `    puntos: [`, one string `      '…',` per line
//   (`\'` for an apostrophe), `    ],` and `  },`. Same views and same number of points as
//   ayuda.es.js. Screens and buttons quoted between “ ” must exist verbatim in en.js
//   (the test checks it); anything else, such as commands, goes between ‘ ’.

export default {
  resumen: {
    intro: 'At a glance: how each cloned account is doing and whether GitHub is working.',
    puntos: [
      'Each card is a GitHub account. The colour shows its worst repository: green, everything up to date; amber, something has not synced for a while; red, something is failing.',
      'The counters separate the repositories that are up to date, those that need attention, those that no longer exist on GitHub (they are kept here) and those in contingency.',
      '“Sync now” forces a pass without waiting for the timer. While it runs you will see a bar; afterwards, ‘Cloning repositories: X of N’ shows how many Gitea has finished fetching.',
      '“Open Gitea” opens that account’s local server in the browser. It will ask for a username and password: you will find them under “Username and password”.',
      'If the GitHub token is about to expire, a warning appears: renew it in “Settings” → “Rotate token”.',
      'The indicator at the top right shows the state of GitHub. If it is down, go to “Contingency”.',
    ],
  },
  repositorios: {
    intro: 'The full list of an account’s repositories and their status.',
    puntos: [
      'Filter by status or search by name. Problems are listed first.',
      '“On GitHub” shows whether the repository is public or private there. Your local copy is always private: nobody sees anything in Gitea without signing in.',
      'Those marked “not cloned” exist on GitHub but have no copy: tick them to include them. If they are forks, first turn on “Include forks” in “Settings”.',
      'Untick “Included” to stop cloning a repository: its copy is paused, not deleted.',
      '“Sync” updates only that repository; if its first clone was interrupted, it clones it again. “Open” shows it in the local Gitea.',
      '“Orphan” means it is no longer on GitHub. gitmereba never deletes an orphan: you decide what to do with it.',
      'The clone address is that of your local copy; it works even when GitHub is not responding.',
    ],
  },
  contingencia: {
    intro: 'To keep working when GitHub is not responding and send your changes back afterwards.',
    puntos: [
      '“Activate contingency” creates a writable copy of the repository in your local Gitea and pauses its sync. The original is left untouched.',
      'Copy the ‘git remote add mereba …’ command into your working folder and send your changes with ‘git push mereba’.',
      'When GitHub is back, press “Reconcile” and enter a token with write permission. It is used only for that push and is not stored.',
      'Nothing is ever forced: if GitHub received different changes in the meantime, the reconciliation stops and tells you, so you can sort it out yourself with git.',
      'After reconciling, the repository goes back to syncing as normal.',
    ],
  },
  actividad: {
    intro: 'What gitmereba has done and when.',
    puntos: [
      '“History” lists every pass: how many repositories were created, how many failed and a summary.',
      '“Audit” records every important action (accounts added or removed, exclusions, contingencies, settings changes). It is append-only: it cannot be edited or deleted.',
      'Each entry is chained to the previous one with a fingerprint. “Chain intact ✓” confirms that nobody has tampered with the record.',
    ],
  },
  ajustes: {
    intro: 'Settings for each account and for the application.',
    puntos: [
      '“Sync interval (minutes)”: how often the account is synced (minimum 10 minutes). It works even when this window is closed.',
      'Forks and organisations: what is cloned besides your own repositories. After turning on “Include forks”, tick the ones you want in “Repositories” and sync.',
      '“Rotate token” replaces the GitHub token. It is stored in the system keyring, never in files.',
      '“Local network access” shares the account’s Gitea with other computers on your network, always over HTTPS. It is off by default; follow the three steps it shows when you turn it on.',
      '“LAN users” creates people who sign in with their own username instead of the administrator: they can read all the mirrors and write only to the contingency repositories. Gitea generates the password and shows it only once, when the user is created: copy it then, because gitmereba does not store it. If someone loses it, delete their user and create it again.',
      '“Remove account…” stops the sync but does not delete the repositories already cloned.',
      '“Update (verified)” downloads the Gitea version pinned by gitmereba and checks its signature before installing it.',
    ],
  },
  alta: {
    intro: 'Three steps to clone a GitHub account.',
    puntos: [
      '“GitHub username”: the name of the GitHub account you want to clone.',
      '“Read token (PAT)”: create it on GitHub → Settings → Developer settings → Fine-grained tokens, with read-only access to “Contents” and “Metadata”. It is stored in the system keyring.',
      '“Destination folder”: it must be empty or not exist. Everything for the account will live there (repositories, Gitea and backups); you can move or back it up as a whole.',
      'Before anything is created you will see which repositories are going to be cloned and how much space they take; untick the ones you do not want.',
      'If something fails halfway, repeat the sign-up: it carries on where it left off.',
    ],
  },
};
