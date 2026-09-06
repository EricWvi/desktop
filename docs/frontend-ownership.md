# Frontend ownership

## Translation resources

Feature copy lives in data-only `features/<feature>/translations.ts` entries. Settings splits
general settings, plugins, Skills, and Roles behind its data-only translation entry; workflow
editor copy belongs to the editor even when its historical keys start with `settings.workflow`.
Domain-specific contract errors live beside the corresponding feature's copy. Only truly shared
shell/transport messages remain in `i18n/common-resources.ts`.

`i18n/resources.ts` explicitly composes those entries. It imports no React or i18next integration,
and resources never import their feature implementation. `composeTranslationResources` rejects
duplicate logical-key ownership, missing language keys, and incomplete plural groups. English
`_one` / `_other` variants and Chinese unsuffixed count messages retain their original keys; the
check compares logical keys instead of demanding identical raw plural suffixes.

`i18n/i18n-instance.ts` is still the sole `appI18n` initializer. It composes all copy synchronously
(`initAsync: false`), retains Chinese fallback, and preserves `ora.locale` storage and document
language behavior. Features do not dynamically register resources when mounting. Tests that render
`useTranslation` import `appI18n` themselves rather than depending on another file in the worker.

Adding or removing a feature changes its resource entry and the explicit composition. Editing
existing copy changes only its owner. `resources.test.ts` exercises data-only composition,
ownership collisions, language parity, and plural validation; `i18n-instance.test.ts` covers
synchronous availability, language switching, plurals, and blocked storage.
