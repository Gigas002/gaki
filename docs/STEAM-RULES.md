# Steam rules and risks

> **This is a technical review, not legal advice.** It compares what gaki
> does with the text of Valve's published rules, as read on the date below.
> It can't say how Valve enforces them, and nobody working on gaki is a
> lawyer. Use gaki at your own risk.

gaki is an independent, unofficial project. It is not affiliated with,
authorized, sponsored or endorsed by Valve or Steam. "Steam" and "Valve" are
trademarks of Valve Corporation.

Review date: **2026-11-08** (PLAN.md Phase 9). This page is the working
audit (steps 9.1 and 9.2). Step 9.4 turns it into the user guide.

## 1. The rules (9.1)

Every page was read on 2026-11-08, in English.

| Rule | Link | Version | Applies to |
| ---- | ---- | ------- | ---------- |
| Steam Subscriber Agreement (SSA) | <https://store.steampowered.com/subscriber_agreement/> | last updated 2026-09-10 | **Users.** Each account holder accepts it. gaki isn't a party, but what it does, it does as the user. |
| Steam Online Conduct | <https://store.steampowered.com/online_conduct/> | undated | **Users** (SSA §4.A makes it binding). |
| Steam Web API Terms of Use | <https://steamcommunity.com/dev/apiterms> | last updated July 2010 | **Neither, mostly.** It covers holders of a Web API key, and gaki has none. gaki does call three keyless `api.steampowered.com` methods (§2.11), so it follows the spirit: no password storage, no implied endorsement, far under 100,000 calls a day. |
| Steam brand guidelines | <https://partner.steamgames.com/doc/marketing/branding> | undated | **Neither, formally.** They are written for Steamworks partners. Used here as guidance for gaki's own names and art. |
| Valve legal notices (trademarks) | <https://store.steampowered.com/legal/> | © 2026 | **gaki.** Lists "Steam" and "the Steam logo" as Valve trademarks. |
| Steamworks / Steam Distribution Agreement | <https://partner.steamgames.com/steamdirect> | undated | **Neither.** It binds developers who publish on Steam. gaki publishes nothing. |
| Community Market and trading terms | SSA §3.C (Wallet), §3.D (trading), §4.B | as SSA | **Users.** gaki only reads the market, inventory and wallet, and never moves money or items (PLAN.md Non-goals). |

The robots.txt files of the hosts gaki contacts were checked on the same
date. They are meant for crawlers, and gaki isn't one: it makes one request
when the user asks. store.steampowered.com and steamcommunity.com disallow
none of the paths gaki uses. api.steampowered.com disallows `/` for every
crawler. www.protondb.com allows everything to ordinary agents.

### Clauses quoted below

- **SSA §1.C (Your Account):** "You may not reveal, share or otherwise allow
  others to use your password or Account except as otherwise specifically
  authorized by Valve." "Your Account … is strictly personal."
- **SSA §2.A (General License):** "Steam and your Subscription(s) require the
  download and installation of Content and Services onto your computer. …
  you may be required to be running the Steam client and maintaining a
  connection to the Internet."
- **SSA §2.G (Restrictions):** "you may not … reverse engineer, derive source
  code from, modify, disassemble, decompile … the Content and Services or
  any software accessed via Steam without the prior consent, in writing, of
  Valve." And (ii): you may not "host or provide matchmaking services for
  the Content and Services or emulate or redirect the communication
  protocols used by Valve in any network feature of the Content and
  Services, through protocol emulation, tunneling, modifying or adding
  components to the Content and Services, use of a utility program or any
  other techniques …, for any purpose including, but not limited to network
  play over the Internet …".
- **SSA §3.A:** "You agree that you will not use IP proxying or other methods
  to disguise the place of your residence, whether to circumvent
  geographical restrictions on game content, to order or purchase at
  pricing not applicable to your geography, or for any other purpose."
- **SSA §4.B (Cheating):** "You agree that you will not tamper with the
  execution of Steam or Content and Services unless otherwise authorized by
  Valve. You may not use Cheats, mods, hacks, or any other unauthorized
  third-party software, to modify any Subscription Marketplace process, the
  process of Steam account creation or otherwise in interacting with or
  controlling the processes or user interface of Steam Content and
  Services, except to the degree expressly permitted."
- **SSA §4.C (Automation):** "You may not use any form of scripts, bots,
  macros, or other non-human-controlled systems ("Automation") to interact
  with Content and Services on Steam in any manner, including but not
  limited to: Automating the Steam account creation process, Faking
  gameplay statistics (e.g., inflated wins or losses, XP, playtime), Earning
  rewards or progress without genuine user input, Participating in
  adjudication systems … through automated means".
- **SSA §4.D:** "Valve may restrict or terminate your Account(s) or a
  particular Subscription for any conduct or activity that is illegal,
  constitutes a Cheat or Automation or breaches the Steam Online Conduct
  Rules. … Valve is not required to provide you notice".
- **Online Conduct:** "Disrupt, damage, or manipulate Steam: … instituting an
  attack on a Steam server; artificially manipulating Steam reviews …".
  "Harass other users …: … spamming". "Improperly gather or distribute
  information: harvesting or otherwise collecting information about
  others". "Violate others' intellectual property rights: … using another's
  trademark or copyrighted material".
- **Web API Terms §2:** "you will not intercept or store the end user's Steam
  password on log in." "You may not present the Steam Data … so that it
  appears (a) that your Application is endorsed or affiliated with Valve or
  Steam". "You may not use the Steam Web API in any way that degrades the
  operation or performance of Steam". "You are limited to one hundred
  thousand (100,000) calls to the Steam Web API per day."
- **Brand guidelines:** "The Steam logo may not be used in any manner that
  might imply that any non-Valve materials are sponsored, endorsed,
  licensed by, or affiliated with Valve." "The Steam trademarks may not be
  displayed as primary or prominent features on any non-Valve materials."

## 2. Feature by feature (9.2)

Ratings: **OK** means no clause speaks against it. **Gray area** means a
clause can be read to cover it, but its wording points elsewhere, or the
practice is common and has been tolerated. **Breaks a rule** means the
plain text forbids it. Nothing gaki does landed in the last group.

The widest clause is SSA §4.B: "unauthorized third-party software …
interacting with … Steam Content and Services". Read literally, it covers
every unofficial Steam client, so every networked feature below inherits
the gray area of 2.1. The ratings below say what each feature adds on top
of that.

| # | Feature | Rating | Clauses | Why |
| - | ------- | ------ | ------- | --- |
| 2.1 | Logging in as an unofficial client: the CM protocol through steam-vent, password, QR and Steam Guard | **Gray area** | SSA §4.B; §2.G(ii); §2.G (reverse engineering) | No written consent from Valve covers it. §2.G(ii) is about multiplayer and matchmaking ("network play"), but its "emulate … the communication protocols used by Valve … use of a utility program" is broad. gaki does no reverse engineering itself; it uses steam-vent's protocol work (§2.G binds the person who reverse-engineers). In gaki's favour: it uses Valve's own authentication service (the same `IAuthenticationService` calls, QR flow and Steam Guard as the official client), never stores the password (only the refresh token, in the OS keyring), and since 8.21 each process logs on with its own logon ID, so it doesn't kick the official client off. **Note:** the logon names the device "DESKTOP-VENT" (steam-vent's default `ClientInfo`), with the Steam Client platform type. That name shows in the account's authorized-devices list; it isn't misleading, but it doesn't say "gaki" either. |
| 2.2 | The session daemon holding one logged-on session | **Gray area (low)** | as 2.1 | It adds nothing new: one session, as the official client keeps one. It shares a connection between commands instead of logging on for each one, which means fewer logons. |
| 2.3 | Depot and manifest downloads, installs, updates, verify | **Gray area (low)** | SSA §2.A; §4.B | §2.A expects Content to be downloaded and installed, and says you "may be required to be running the Steam client". Steam's CDN only hands out depot keys and manifest codes for licences the account owns, so gaki can't fetch anything the user doesn't own. Downloads have no rate of their own beyond the official client's (AUDIT.md §2). |
| 2.4 | Downgrades: older manifests, pinning | **Gray area (low)** | as 2.3; §2.A ("you consent to such automatic updating") | §2.A has the user consent to automatic updates. Pinning declines them for owned content. The official client can do the same through its own `download_depot` console command and the "only update when I launch" setting. Online games may refuse an old build; that's the game's rule, not Steam's. |
| 2.5 | Installing outside the official client: writing `appmanifest_*.acf`, `libraryfolders.vdf`, `appworkshop_*.acf` | **Gray area (low)** | SSA §4.B ("tamper with the execution of Steam") | gaki edits Steam's data files, not the running client. Before editing the files Steam rewrites, it makes sure Steam isn't running, and stops it only when the user asks (`ops::steam`). The files stay in Steam's own format, so the official client reads them as its own installs. |
| 2.6 | Steam Cloud sync | **OK** (plus 2.1) | — | It uses the official Cloud service over the CM for the user's own saves, the same calls as the client. |
| 2.7 | Presence, friends, chat (`friends`, `chat open`, TUI chat) | **OK** (plus 2.1); **gray for scripted use** | Online Conduct ("spamming", "harvesting"); SSA §4.C | Every message, friend request and status change comes from a user action. gaki has no bulk send or auto-reply. But `chat open --json` and its stdin are made for drivers, and a script that answers chats is "Automation … to interact with Content and Services". That's a user choice; gaki should say so (§3, item D). |
| 2.8 | Store data: prices, search, deals, events, players, similar, wishlist, news, reviews, requirements | **OK** | Web API Terms §2 (rate, endorsement); Online Conduct ("attack on a Steam server") | Most of it comes over the CM through official service methods. The rest uses public JSON endpoints the store's own pages use (`appdetails`, `appreviews`, `GetNewsForApp`), with no key, one request per user action, 150 ms between per-region price calls, and 429/`Retry-After` respected (`core::net`). Wishlist add and remove are single user actions. |
| 2.9 | Regional price comparison (`price --compare`) | **OK** | SSA §3.A | It asks the store for another country's price with the `cc` parameter. gaki doesn't hide the user's location or buy anything. **But** gaki has a general `proxy` setting (`config.toml`); one that changes the apparent country, combined with buying at that price, is exactly what §3.A forbids. gaki never buys, so it can't do that itself; worth a sentence in the docs (§3, item E). |
| 2.10 | Market, inventory, wallet, listings (read-only) | **Gray area** | SSA §4.B ("modify any Subscription Marketplace process"); §4.C | Read-only, so nothing in a marketplace process is modified, and there is no buy or sell path (see the "never moves money" policy). But `market/priceoverview`, `market/search/render`, `market/mylistings` and `inventory/` are undocumented web endpoints that Valve limits hard. A script polling them is the classic market bot (Automation). gaki makes one request per command and has no watch or poll mode, but it doesn't cache either. |
| 2.11 | Web session and tokens: OpenID sign-in, `chat/clientjstoken`, `GenerateAccessTokenForApp`, keyless `api.steampowered.com` (`GetCMListForConnect`, `GetServersForSteamPipe`, `GetNewsForApp`) | **OK** (plus 2.1) | Web API Terms §2 (password) | OpenID 2.0 is Valve's documented sign-in for third parties. The web token is minted from the user's own CM session, as the official client mints its own. No Web API key is used or shipped. The password is never stored or sent anywhere but Steam's RSA-encrypted logon. |
| 2.12 | Achievements and stats (read), playtime | **OK** | SSA §4.C ("Faking gameplay statistics … playtime", "Earning rewards … without genuine user input") | gaki reads achievements and the playtime in `localconfig.vdf`. It never sets achievements or stats, and never reports a game as played unless the game runs (no idling). Looping `gaki-cli play` from a script to farm playtime or cards would be the user's Automation; worth a sentence (§3, item D). |
| 2.13 | Workshop: subscribe, install, uninstall, comment | **OK** (plus 2.1) | Online Conduct | Each is a user action. Comments are the user's speech under the conduct rules. |
| 2.14 | Launching: bridging to the host Steam client, standalone (`steam = none`, or `auto` with Steam not running), `steam_appid.txt` | **Gray area (low)** | SSA §4.B ("tamper with the execution of … Content and Services"); §2.A ("you may be required to be running the Steam client") | **Bridged** (`host`, or `auto` with Steam running): gaki starts the game itself, with `SteamAppId` set; for Proton it points `STEAM_COMPAT_CLIENT_INSTALL_PATH` at the real Steam install, so `lsteamclient` reaches the logged-on client. Steamworks, ownership checks and the overlay work as under Steam. **Standalone:** nothing reaches Steam. `STEAM_COMPAT_CLIENT_INSTALL_PATH` points at a "trap" folder whose `steam` and `steam.sh` do nothing (`core::utils::setup_fake_steam_trap`), and the Wine DLL overrides turn off `steamclient`, `steam_api` and `lsteamclient`. A game that asks Steam to start it gets no answer, and `SteamAPI_Init` fails, so a game that enforces Steam (SteamStub DRM, a fatal init check) still can't run. It isn't a DRM bypass: nothing emulates Steam or fakes ownership. What does run is a game whose own code tolerates having no Steam, as with launching its binary by hand. **`steam_appid.txt`:** gaki writes the app id into the game's working folder on every native and Wine launch, overwriting any file the game shipped (`steam_client/process.rs`, `infra/runners/wine_tkg.rs`), and leaves it there. Valve documents the file for developers: with it, `SteamAPI_RestartAppIfNecessary` doesn't quit the game to relaunch it through Steam. In bridged launches that is probably what keeps the game in gaki's pipeline (its Proton, env and profile) instead of being restarted by Steam; whether `SteamAppId` alone is enough wasn't tested. It never makes an unowned game run (§3, item C). |
| 2.15 | DRM ticket check (`drm`) | **OK** (plus 2.1) | — | It requests the app ownership and encrypted app tickets the official client requests, for games the account owns, and only reports them. |
| 2.16 | Proton installs | **OK** | as 2.3 | The official Proton builds are free Steam apps, installed like any other. |
| 2.17 | ProtonDB | **OK** | ProtonDB robots.txt (allows) | The TUI fetches one summary for the highlighted game, once, after input settles, and shows "ProtonDB" next to it. It's an unofficial but public endpoint with no published terms. |
| 2.18 | SteamDB, Steam store, ProtonDB links | **OK** | — | Links only; the browser opens them. |
| 2.19 | Art caching (headers, capsules, avatars) | **OK** | SSA §2.A ("personal, non-commercial use"); Online Conduct (IP) | Public Valve CDN images, cached in the user's config directory for that user's own display. They aren't redistributed. `gaki-cli clean --images` removes them. |
| 2.20 | Names and logos | **OK**, one **gray area (low)** | Legal notices; brand guidelines; Web API Terms §2 | gaki ships no Steam or Valve logo. "Steam" appears as a plain description ("a command-line Steam client"), which is how the README's notice describes a compatible product, and the README states gaki isn't affiliated. The one gray spot is the crate name `gaki-steam-cdn` (inherited from `steam-cdn`): "Steam" is in a published name, though not as the leading or prominent word. It only matters once the crates go to crates.io. |
| 2.21 | The `proxy` setting | **OK** as a feature; see 2.9 | SSA §3.A | Proxies have ordinary uses (corporate networks). The clause covers disguising residence, which is the user's choice. |
| 2.22 | Several accounts (`--config-dir`), account data | **OK** | SSA §1.C | Each config directory holds one person's session. gaki has no account sharing or switching between people. |

## 3. Gray areas to decide (input to 9.3)

For each one, a way to keep the feature closer to the rules. The user picks
keep, change or gate; each change becomes its own 9.x step.

Decided on 2026-11-08: **A** name the device "gaki"; **B** cache plus a
minimum interval, kept on; **C** keep an existing `steam_appid.txt`, write one only
when none exists; **F** keep the name, noted here.
**D** and **E** are docs only and go into 9.4.

| Item | Feature | Proposal |
| ---- | ------- | -------- |
| A | 2.1 Device name "DESKTOP-VENT" | Name the device honestly with `ClientInfo::new("gaki")`. The authorized-devices list then says which program holds the session, so the user can revoke it. It doesn't change the platform type or the logon. |
| B | 2.10 Market and inventory endpoints | Add a short cache (for example 5 minutes for `price`, 1 minute for `inventory`) and a minimum interval between market calls in one process. Keep it on: it's read-only and user-driven. Alternative: put it behind a feature gate that's off by default. |
| C | 2.14 `steam_appid.txt` | Keep the file if the folder has one (the developer's, or gaki's from an earlier launch); otherwise write the game's own id. Never overwrite, never delete. Steam doesn't write this file itself (it passes `SteamAppId` and `SteamGameId` instead), so one already in the folder is either the developer's, which then wins, or one gaki wrote with the same id. A game update that ships one replaces it through Steam's own download. |
| D | 2.7, 2.12 Scripted chat and play | No code change. Say in the docs (and the `chat open --json` help) that scripted replies and looped launches are Automation under SSA §4.C and the user's responsibility. |
| E | 2.9, 2.21 Proxy and regional prices | No code change. One sentence in the proxy docs: SSA §3.A forbids using a proxy to disguise your country. |
| F | 2.20 `gaki-steam-cdn` crate name | Decide before publishing to crates.io: keep it (it follows `steam-cdn` and `steam-vent`), or rename it (for example `gaki-cdn`). |

Not gray, so no change proposed: the session daemon (2.2), downloads and
downgrades (2.3, 2.4), Steam file edits (2.5) and the general §4.B reading
(2.1). They are what gaki is for. 9.4 should state them plainly as the
risk of using any unofficial client: Valve can restrict an account without
notice (SSA §4.D). How likely that is can't be judged from the text.
