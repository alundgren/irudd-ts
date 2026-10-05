# Recipe book UX

The reader's task is to find a check they can use and understand its result in
a few minutes. The index is for lookup. Each recipe is for one task. Detailed
justification belongs in the linked reference, not the cookbook.

## Navigation and content

The first check is the main entry point. The index groups task names into
source setup, dependency rules, file conventions, extensions, and review
evidence. A search matches prose, rule names, configuration keys, and code.
Space-separated terms combine. Category buttons narrow the results. Query and
category persist in the URL, and an empty result offers a reset.

Recipes use one order: task, prerequisites, visual relationship, named files
and examples, command, expected result, one short evidence limit, references,
related tasks. Show complete miniature projects for policies. Label excerpts
and fragments explicitly. Copy controls preserve the exact source text and
select it for manual copying if clipboard access fails.

All content is static HTML. JavaScript adds search and copying. Readers without
scripts can browse every task or use browser Find. Agents get an HTML map,
`llms.txt`, a versioned JSON index, and Markdown versions of every recipe. These
copies are generated from the same maintained content.

## Color and surfaces

Use the house warm-paper palette exactly. Apply a color only when it explains
a relationship or an outcome. Categories share one style; they do not receive
different colors. Status meanings always have visible words and distinct line
treatments, so color is never the only signal.

| Role | Value | Use |
| --- | --- | --- |
| Background | `#F2EADE` | Reading ground |
| Surface | `#EADFCD` | Code examples, diagram nodes, hover rows |
| Raised | `#E0D2BD` | Selected category, compact brand mark |
| Line | `#C1AF9A` | Separators and neutral relationships |
| Field | `#F9F6F0` | Search input and action text |
| Text | `#604939` | Main prose and diagram labels |
| Muted | `#66574D` | Captions, labels, and secondary metadata |
| Accent | `#784F26` | First-check action, numeric syntax |
| Link | `#3D5D71` | Links, graph input, JSON keys and code keywords |
| Success | `#3D6034` | Allowed paths and clean results, string syntax |
| Warning | `#7E5220` | Incomplete results and qualifications |
| Danger | `#8F3A2D` | Blocked paths and policy violations |

Syntax colors distinguish token kinds inside code; status colors distinguish
outcomes inside diagrams. Keep those meanings local to their context.
Use one lightness step for a code panel. No shadows or nested cards.
Search is the lightest region. The recipe index uses open rows and generous
spacing rather than a uniform grid of panels.

## Type and space

Self-host IBM Plex Sans and Mono with their OFL notice. Use weights 400, 500,
and 600 only. Body text is 16px with a 1.6 line height. Titles range from 28px
to 40px. Sections are 22px; compact headings are 17px. Secondary labels use
13.5px to 14px. Uppercase labels are 11px with 0.07em tracking.

Cap prose around 70 characters. The index is wider for lookup; recipe pages
are narrower for reading. Give code its own scroll region when a line cannot
fit. Keep the document itself within the viewport. On phones, diagrams stack
their labels in reading order rather than shrinking text.

## Diagrams

Use hand-authored SVG markup and CSS, never Mermaid. Two to four rounded nodes
show one relationship. Neutral arrows indicate a path or processing order;
labels identify the actual source, command, or result. Allowed paths use sage.
Blocked paths use clay and dashed outlines. Incomplete results use pear and
dotted outlines. Unrelated result states do not have connecting arrows.

SVG labels are real text, with an accessible description and adjacent caption.
The stacked mobile version shows the same labels. Do not infer runtime
execution from a source dependency arrow.

## Tone

Name the task with a verb. Name the exact file above a sample. Give a complete
command with the working directory and prerequisites. Say what finding appears
and what correction clears it. Keep product qualifications close to the example.
Use no sales copy, invented performance claims, score targets, or deep rationale.
Use sentence case and plain, direct language.

## Review

Check desktop and phone layouts, long source paths, zero search results, direct
links with queries, category changes, keyboard navigation, copy failure, and
no-script reading. Verify the real `/irudd-ts/` project prefix. Check code text
against generated Markdown and exercise every policy's displayed failure,
correction, and relevant negative control with the actual CLI.
