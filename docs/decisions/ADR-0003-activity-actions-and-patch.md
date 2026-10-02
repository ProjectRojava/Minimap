# ADR-0003: Extra activity actions and `Patch<T>` for updates

- **Activity actions**: `CLAUDE.md` §4.3 lists `created, updated, archived, edge_added, edge_removed`. We add `unarchived` and `deleted`: restoring and permanently deleting are state changes the history view and undo (feature 25) need to see, and activity rows outlive deleted nodes.
- **`Patch<T>`** (`keep` / `set` / `clear`) expresses updates to nullable fields, since `Option<T>` cannot distinguish "not provided" from "set to null" over JSON. Non-nullable fields use `Option<T>`.
- **`completed_at`** is not patchable: the store sets it when a task enters `done` and clears it on leaving.
