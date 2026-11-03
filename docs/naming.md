# Naming research

Checked 2026-11-03. Recommendation: **Reinrelay**, binary `reinrelay`. A rein evokes the harness; a relay carries requests onward. It is pronounceable as “rein relay,” uses familiar words, and suggests a network hop without promising a full OpenAI implementation. The repeated `r` is slightly awkward and “rein” can be mistaken for “rain,” so always show the spelling alongside the name. Suggested description: **“An authenticated API for your coding harnesses.”**

Example shape, if chosen later: `reinrelay keys create`, `reinrelay serve --provider codex`, `reinrelay doctor`. Use `reinrelay` consistently as brand, binary, repository slug, and an unscoped npm package only if a subsequent check and the owner’s distribution plan support it. A scoped npm wrapper is another option, but its scope and package have not been checked. These are proposals, not installed commands or registrations; the current binary is `h2a`.

## Shortlist

Brand and proposed executable are the same lowercase spelling in every row. GitHub results below are repository **name search** counts, not ownership or an exhaustive code search. npm results are exact unscoped package registry HTTP responses. Links reproduce the checks; results can change.

| Rank | Brand / binary | Why it works; tradeoff | GitHub name search | npm exact package |
| --- | --- | --- | --- | --- |
| 1 | Reinrelay / `reinrelay` | Harness metaphor plus network relay; concise, but requires spelling out “rein.” | [0 results](https://github.com/search?q=reinrelay%20in%3Aname&type=repositories) | [404](https://registry.npmjs.org/reinrelay) |
| 2 | HarnessJet / `harnessjet` | Clear anchor and energetic; “jet” may imply speed. | [0 results](https://github.com/search?q=harnessjet%20in%3Aname&type=repositories) | [404](https://registry.npmjs.org/harnessjet) |
| 3 | Reinport / `reinport` | Short, suggests a port for a harness; “rein” needs explanation and the name exists outside software. | [2 substring results](https://github.com/search?q=reinport%20in%3Aname&type=repositories), both portfolio names in the API response | [404](https://registry.npmjs.org/reinport) |
| 4 | HarnessPier / `harnesspier` | A place to connect harnesses; easy to pronounce, but longer and the pier metaphor is indirect. | [0 results](https://github.com/search?q=harnesspier%20in%3Aname&type=repositories) | [404](https://registry.npmjs.org/harnesspier) |
| 5 | Bridlet / `bridlet` | A compact coined form of bridle; memorable but less descriptive and potentially heard as “bridle it.” | [2 substring results](https://github.com/search?q=bridlet%20in%3Aname&type=repositories), both Bridle Trails names in the API response | [404](https://registry.npmjs.org/bridlet) |
| 6 | Reinbridge / `reinbridge` | Conveys connection; longer, and [ReInBridge is already an unrelated project](https://avkc.lt/en/project-development-administration/). | [0 results](https://github.com/search?q=reinbridge%20in%3Aname&type=repositories) | [404](https://registry.npmjs.org/reinbridge) |

A zero-result search or npm 404 means only that the particular query returned no public match at that time. It does **not** establish that a name is available, that a binary will not conflict on a user's PATH, or that any trademark, domain, organization, registry, or platform rights are clear. GitHub slugs are owner-scoped: a matching repository name elsewhere is a discoverability concern, not a global reservation. An npm unscoped package name is global to that registry. Binary names are local to each installation and may be supplied by unrelated packages. Recheck the final choice, relevant package managers and target organization before release; conduct appropriate trademark and domain review separately. No trademark availability is claimed.

## Names rejected for collisions

- **HarnessPort** has an [AI coding harness converter repository](https://github.com/calghar/harnessport) and [an exact npm package](https://registry.npmjs.org/harnessport), making confusion particularly likely.
- **Harnest** has an [AI coding assistant configurator](https://github.com/alcherk/harnest) and [an exact npm package](https://registry.npmjs.org/harnest).
- **HarnessDock** already names [a DeepSeek Harness desktop workspace](https://github.com/nortonyang/HarnessDock) and [another manager](https://github.com/cloudyview/harnessdock), even though the exact npm registry request returned 404.
- **AgentQuay** has [an agent-related bridge](https://github.com/wenbaow/agentquay) and [an exact npm package](https://registry.npmjs.org/agentquay).

## Positioning against adjacent tools

[CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI) advertises several provider-compatible interfaces and coding subscriptions. [Star CLIProxy](https://github.com/starhunt/star-cliproxy) explicitly advertises local compatible endpoints backed by multiple coding CLIs. [codex-proxy](https://github.com/Max-Leopold/codex-proxy) describes a local OpenAI-compatible proxy backed by Codex CLI login. This is an established category, so avoid implying the idea or CLI-backed approach is unique.

The local project's narrower, verifiable positioning is: an authenticated, text-completion compatibility layer that invokes installed official Codex and Antigravity CLIs, with per-key model scopes and a CLI-first Go distribution. The README describes the supported endpoints and limitations; `internal/harness/process.go` invokes the executable and `internal/api/server.go` handles completion routes. Do not imply full OpenAI API compatibility, independent provider credentials, guaranteed account eligibility, or isolation equivalent to a VM. Comparisons describe project documentation, not an independent feature or security audit of other projects.

## Method and limits

On 2026-11-03, queried `https://api.github.com/search/repositories?q=NAME+in:name&per_page=5` and `https://registry.npmjs.org/NAME` for each shortlist spelling. For GitHub, the API reported counts of 0, 0, 2, 0, 2, 0 in shortlist order; for npm each returned HTTP 404. Also searched the web for quoted names and inspected the linked adjacent projects. For example, a web search found unrelated uses of [Reinport](https://companieshousecyprus.com/company/reinport-management-limited-he191213) and Reinbridge. Search engine indexing and GitHub repository-name search do not cover all software or uses. Later exploratory GitHub requests hit an unauthenticated rate limit; those errors were not counted as zero results. No domain, operating-system package registry, or trademark database was checked, and nothing was reserved.
