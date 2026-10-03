# gitmereba manual

[Español](manual.md) · **English**

gitmereba keeps a live copy of your GitHub accounts on your computer. If GitHub goes down, you
carry on working against your copy and, when it comes back, you send the changes back.

Everything happens on your machine: each account has its own local Gitea server, tokens are
kept in the system keyring and nothing is deleted unless you decide so.

## 1. Requirements

- Linux with a desktop (tested on Ubuntu), with `git` and `gpgv` installed.
- An active system keyring (GNOME Keyring or KWallet).
- A **read-only** GitHub token: GitHub → *Settings* → *Developer settings* →
  *Fine-grained tokens* → repository permissions **Contents: Read** and **Metadata: Read**.

## 2. Installation

Download the `.deb` from the [releases](https://github.com/jparga/gitmereba/releases) page and
install it with `sudo apt install ./gitmereba_<version>_amd64.deb`. To build it yourself,
install without administrator rights or pin gitmereba to the Ubuntu dock, see
[`compilar.md`](compilar.md) (in Spanish).

## 3. Getting started

Open gitmereba (`gitmereba` with no arguments) and press **Add account**:

1. GitHub **username**, **token** and destination **folder** (empty or non-existent).
2. Review the list of repositories that are going to be cloned and untick any you do not want.
3. Press **Create account**. gitmereba downloads and verifies Gitea, configures it and creates
   the copies.

If something fails halfway, repeat the account setup: it carries on where it left off. You can add as
many accounts as you like; each one is independent.

From then on the account syncs by itself every 30 minutes, **even with the window closed**, and
you get a desktop notification if something fails.

## 4. The screens

Every screen has its own help: **How does this screen work?** under the title, or the **F1**
key.

| Screen | What it is for |
|---|---|
| **Summary** | Status of each account and of GitHub. Sync now. Open Gitea. |
| **Repositories** | List with the status of each repository. Include or exclude. Local clone address. |
| **Contingency** | Work without GitHub and send the changes back afterwards. |
| **Activity** | Sync history and audit log. |
| **Settings** | Interval, scope, token, local network access, removal, Gitea update. |

### Signing in to Gitea

**Open Gitea** opens the account's local server in your browser and asks for a username and
password. You will find them under **Summary → Username and password**: the username is
`gitmereba-admin` and the password is random, stored in your system keyring. It is an
administrator account: do not share it.

### Progress

When you press **Sync now** you will see a bar while gitmereba talks to GitHub. If there are new
repositories, **Cloning repositories: X of N** appears afterwards until Gitea has finished
fetching them; large ones can take several minutes.

### Public, private and forks

The **On GitHub** column in Repositories says whether the repository is public or private *on
GitHub*. Your local copy is **always private**: nothing in Gitea can be seen without signing
in, even if the original is public.

Forks are not cloned unless you ask for them. They appear in the list as **not cloned**. To
bring them in: Settings → **Include forks** → Save; then tick the ones you want in Repositories
and press **Sync now** in Summary.

### Repository statuses

| Status | Meaning | What to do |
|---|---|---|
| **OK** | The copy matches GitHub | Nothing |
| **Outdated** | It has gone too long without syncing | Press “Sync”; if it persists, look in Activity |
| **Failed** | The copy fails or does not match GitHub | Read the repository's details |
| **Orphan** | It no longer exists on GitHub | It is kept paused; you decide |
| **Contingency** | It has a writable copy in use | Reconcile when GitHub is back |
| **Excluded** | You have unticked it | Include it again whenever you like |

## 5. If GitHub goes down: contingency

1. In **Contingency**, press **Activate contingency** on the repository you need.
2. Copy the command that appears and run it in your working folder:
   ```bash
   git remote add mereba <address shown by the app>
   git push mereba main        # work and push as usual
   ```
3. When GitHub is back, press **Reconcile** and enter a token **with write access**
   (*Contents: Read and write*). It is used only for that push and is not stored.

gitmereba **never forces** a push. If GitHub received other changes in the meantime, it stops
and tells you: merge them yourself with `git pull`/`git merge` and reconcile again.

## 6. Recovering deleted history

Before each sync, gitmereba saves a snapshot of the branches and tags. If someone rewrites
history on GitHub (a *force-push*) or deletes a branch, you get a warning and the previous
version stays saved in `<folder>/snapshots/`. The last 10 snapshots and all of those from the
last 30 days are kept.

## 7. Sharing with other computers on your network

Off by default. In **Settings → Local network access** press **Share on the local network**
(or `gitmereba cuenta lan --login <username> --activar`). The app gives you three things:

1. **The line for `/etc/hosts`** on each computer, for example
   `192.168.1.40  jparga.gitmereba.internal`. On the host computer itself it is
   `127.0.0.1  jparga.gitmereba.internal`.
2. **The certificate** and the command to make `git` trust it for that address only. Copy the
   `.crt` file to the other computer and check that its **SHA-256 fingerprint** matches the
   one the app shows.
3. **The recommended firewall rule** to restrict the port to your network.

The address will be `https://<username>.gitmereba.internal:<port>`. It is always HTTPS.

> While sharing is on, Gitea listens on every network the computer is connected to. Turn it off
> if you take your laptop to a network that is not your own.

### Users for other people

With the LAN on, each person should sign in with their own user, not with `gitmereba-admin`.
Under **Settings → LAN users** (below “Local network access”) you will find the list and two
actions:

- **Create user**: type a name and confirm. A dialogue shows the **username and password**
  that Gitea has generated (32 characters) — **it is shown only once**. It is hidden by
  default; **Show** reveals it and **Copy** puts it on the clipboard. If you close the dialogue
  without noting it down, there is no way to recover it: all you can do is delete the user and
  create a new one.
- **Delete** (with confirmation): removes the user from Gitea. Any commits they have already
  pushed are left untouched.

From the terminal it is the same with `gitmereba cuenta usuario` (see §8).

Each person created this way is a **restricted** Gitea user: they only see what gitmereba gives
them, without needing the administrator account:

- **Read** access to all the mirrors of the regular organisations.
- **Write** access only to the contingency repositories (`contingencia-*`), so they can push
  changes while GitHub is down (§5). A `git push` to a regular mirror is rejected:
  “mirror repository is read-only”.

Nothing else needs turning on: after every sync and whenever a contingency is activated,
gitmereba reconciles the permissions by itself, so a new organisation or contingency
organisation gets its team and all the LAN users. gitmereba **does not store the password**
(not in the keyring, nor in files, nor in the log); the person can change it later from the
Gitea web interface.

**How the other person clones**: they add their `/etc/hosts` line, trust the certificate with
the `git config` command from the LAN panel (see above) and clone with their username and
password:

```bash
git clone https://<host>.internal:<port>/<owner>/<repo>.git
```

Known limitation: a restricted user can create repositories of their own in that Gitea (they
take up local disk space; they are not synced with GitHub or anywhere else).

## 8. Command line

The subcommands and flags are in Spanish and are not translated; only the help and the
messages follow your language (see §12).

```bash
gitmereba                         # opens the window
gitmereba cuenta add --login <username> --carpeta <path>   # asks for the token without echoing it
gitmereba cuenta list
gitmereba cuenta rm <username>                              # does not delete the repositories
gitmereba cuenta lan --login <username> --activar | --desactivar | --estado
gitmereba cuenta usuario --login <username> --crear <name> | --eliminar <name> | --listar
gitmereba sync <username> | --todas [--simulacro]
gitmereba sync <username> --repo <owner/name>               # retries a clone that failed
gitmereba status [--json]
gitmereba doctor                  # checks the system and each account
```

## 9. Security in brief

- Tokens live in the system keyring; never in files, logs or arguments.
- Gitea is downloaded from its official site and its fingerprint and signature are checked
  before it is used.
- Copies are always private, registration is closed and, unless you share it yourself, Gitea
  only listens on your computer.
- Nothing is deleted automatically: neither orphans nor folders when an account is removed.
- The audit log is append-only and hash-chained: **Activity** tells you whether it is intact.

## 10. If something goes wrong

| Symptom | Try |
|---|---|
| I do not know what is failing | `gitmereba doctor`: it says which check fails and how to fix it |
| “The GitHub token is invalid or has expired” | **Settings → Rotate token** |
| “The system keyring is locked” | Unlock the system keyring (sign in to the desktop) |
| A repository in **Failed**: “the mirror has never synced” | Its first clone was interrupted. Press **Sync** on its row: the empty copy is discarded and it is cloned again |
| It does not sync with the window closed | `systemctl --user list-timers \| grep gitmereba` |
| Gitea does not open | `systemctl --user status gitmereba-gitea-<username>` |
| Another computer cannot connect | Check its `/etc/hosts`, the host's firewall and that it uses `https://` and the port |
| Someone forgot their LAN password | It cannot be recovered: **Delete** that user and **Create user** again with the same name |

## 11. Backup and uninstalling

The whole account lives in its folder: copying it in full (with the account stopped) is a
complete backup. To stop using an account, **Settings → Remove account…**; the folder stays
where it is until you delete it yourself.

## 12. Language

gitmereba is available in English and Spanish: the window, the built-in help, the command-line
help and messages, and the notifications.

- **Where the language comes from.** By default it follows your system, using the first of
  `LC_ALL`, `LC_MESSAGES` and `LANG` that is set. If none of them is set, or the value is `C`
  or `POSIX`, gitmereba uses English.
- **Choosing it yourself.** In **Settings → Interface language** pick *Automatic (system)*,
  *Español* or *English*. The choice is saved in `$XDG_CONFIG_HOME/gitmereba/preferencias.toml`
  (by default `~/.config/gitmereba/preferencias.toml`) and takes priority over the system.
- **Checking it.** `gitmereba doctor` shows the language in use and where it comes from
  (the preference file, an environment variable or the default).
- **What stays in Spanish.** The subcommands and flags of the command line
  (`gitmereba cuenta add`, `--activar`). The sync history, the audit log and the logs are also
  stored in Spanish, whatever language the window is in.
