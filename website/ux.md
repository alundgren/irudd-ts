# Site UX

The reader already has a repository and a linter. Their question is whether
Archguard would help them keep quality up and stop the codebase drifting, and
what it adds to Oxlint. The site answers that question and nothing else.
Installation, file selection, options, experiments, and protocol details belong
in the reference. A reader who wants to try Archguard hands those to their agent.

## Navigation and content

The home page is the main page. In order: the claim with one diagram, what Oxlint
already covers next to what Archguard adds, what an exit code means, one real
repository, how to try it with an agent, and a searchable list of examples.
Each "adds" item links to the examples that show it.

Keep sections few and spaced. Do not add foldouts, per-section eyebrows,
"you need" notes, or long lists of caveats. If a point needs a caveat to stay
true, rewrite the point so it is true without one.

An example page has one job: show a problem and what Archguard says about it.
In order: title, one sentence, diagram, config, the offending file, the real
report, the fix, and one link to the reference. Labels above code are captions,
not headings. Examples that are not policy checks show at most two code blocks.

All content is static HTML. JavaScript adds search and copying. Readers without
scripts can browse every example. Agents get `llms.txt`, `examples.json`, and a
Markdown copy of every example, generated from the same content.

## Color and surfaces

Use the house warm-paper palette exactly. Apply a color only when it explains
a relationship or an outcome. Do not color items just to tell them apart. Status meanings always have visible words and distinct line
treatments, so color is never the only signal.

| Role | Value | Use |
| --- | --- | --- |
| Background | `#F2EADE` | Reading ground |
| Surface | `#EADFCD` | Code examples, diagram nodes, hover rows |
| Raised | `#E0D2BD` | Compact brand mark |
| Line | `#C1AF9A` | Separators and neutral relationships |
| Field | `#F9F6F0` | Search input and action text |
| Text | `#604939` | Main prose and diagram labels |
| Muted | `#66574D` | Captions, labels, and secondary metadata |
| Accent | `#784F26` | Main "try it" action, numeric syntax |
| Link | `#3D5D71` | Links, graph input, JSON keys and code keywords |
| Success | `#3D6034` | Allowed paths, passes, "Archguard adds" items, string syntax |
| Warning | `#7E5220` | Incomplete results, surviving mutations |
| Danger | `#8F3A2D` | Blocked paths, policy violations, rule IDs in reports |

Syntax colors distinguish token kinds inside code; status colors distinguish
outcomes inside diagrams. Keep those meanings local to their context.
Use one lightness step for a code panel. No shadows or nested cards.
Search is the lightest region. The example list uses open rows and generous
spacing rather than a uniform grid of panels.

## Type and space

Self-host IBM Plex Sans and Mono with their OFL notice. Use weights 400, 500,
and 600 only. Body text is 16px with a 1.6 line height. Titles range from 28px
to 40px. Sections are 22px; compact headings are 17px. Secondary labels use
13.5px to 14px.

Cap prose around 70 characters. The home page is wider; example pages
are narrower for reading. Give code its own scroll region when a line cannot
fit. Keep the document itself within the viewport. On phones, diagrams stack
their labels in reading order rather than shrinking text.

## Diagrams

Use hand-authored SVG markup and CSS, never Mermaid. Two to four rounded nodes
show one relationship. The home page diagram may add one path line under the
nodes to show what a per-import check misses. Neutral arrows indicate a path or processing order;
labels identify the actual source, command, or result. Allowed paths use sage.
Blocked paths use clay and dashed outlines. Incomplete results use pear and
dotted outlines. Unrelated result states do not have connecting arrows.

SVG labels are real text, with an accessible description. Add a visible caption
only when it says more than the labels.
The stacked mobile version shows the same labels. Do not infer runtime
execution from a source dependency arrow.

## Tone

Lead with what the reader gets. Name the actual problem: client code reaching
the server, a migration nobody registered, a test that passes after the code
changed. Show it in a small example instead of explaining it.

Compare with other tools factually. Say what Oxlint already does well and tell
readers to keep using it for that. Name overlapping tools such as
dependency-cruiser. Make no speed claims, bug counts, or claims that a check is
impossible elsewhere. Write like a colleague, in sentence case and plain words.

## Review

Check desktop and phone layouts, long source paths, zero search results, direct
links with queries, keyboard navigation, copy failure, and no-script reading.
Verify the real `/irudd-ts/` project prefix. Check code text
against generated Markdown and exercise every policy's displayed failure,
fix, and negative control with the actual CLI.
