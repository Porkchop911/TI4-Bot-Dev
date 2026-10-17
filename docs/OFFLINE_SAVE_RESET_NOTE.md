# Note for testers: offline games reset once

The offline save format changed (format 2, shared by both web apps). Offline games saved by an
earlier build are not opened and are not converted. After the update the offline page shows them as
"older version" and offers to delete them; exported game files of the old format are refused on
import. New offline games are saved in the new format and keep working across later updates as
before. Online games (My games) are not affected. If an old offline game matters, finish or note
it before the update; there is no migration.
