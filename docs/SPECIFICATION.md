# Specification

The specification of Hune has eight parts. It describes the product that the code implements, and the rules that the tests check. It is a draft, version 0.1. The [implementation status](implementation-status.md) lists the differences between the code and the text.

The key words MUST, MUST NOT, SHOULD and MAY follow RFC 2119 and RFC 8174 when they are in capital letters.

| Part | Content | Sections |
| --- | --- | --- |
| [Part 1: Overview](01-overview.md) | The purpose, the scope, the decisions and the architecture. | 1 to 3 |
| [Part 2: Document format and storage](02-format-and-storage.md) | The JSON format of a dashboard, the volume and PostgreSQL stores. | 4 and 5 |
| [Part 3: API, query engine and data sources](03-api-query-sources.md) | The HTTP API, the query engine and the three kinds of data source. | 6 to 8 |
| [Part 4: Front end and editor](04-frontend-editor.md) | The page, the panels, the editor, the history and the conflicts. | 9 and 10 |
| [Part 5: Access and security](05-access-security.md) | The access rules, ProcurAuth and the security rules. | 11 and 12 |
| [Part 6: Configuration and operation](06-configuration-operation.md) | The configuration, the observability and the operation. | 13 and 14 |
| [Part 7: Migration, tests and delivery](07-migration-tests-delivery.md) | The import from Grafana, the tests and the delivery. | 15 to 17 |
| [Part 8: Roadmap and open questions](08-roadmap-open-items.md) | The roadmap, the open questions, the glossary and the references. | 18 to 20 |

## Rules of the text

- A numbered paragraph `N.M` keeps its number in all later versions. A paragraph that leaves the text keeps its number with the word "Unused".
- A cross-reference gives a section number (for example "section 11") or a paragraph number (for example "5.14").
- A code span holds an exact name: a key, a member, a route, a type or a command.
- The code and the tests cite the paragraph numbers.

## Other documents

- [Access and ProcurAuth](ACCESS.md) tells the operator how to run the mode `authzen`.
- [Conformance](CONFORMANCE.md) tells how Hune follows the ProcurAuth protocol.
- [Chart engine edits](engine-patches.md) lists the edits of the copied chart engine.
