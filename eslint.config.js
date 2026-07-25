// citrate-quorum — eslint flat config.
//
// Why this exists: QRM-S4's retro carried an action item to adopt eslint and
// delete `scripts/check_hooks_after_return.py`, a hand-rolled tripwire whose
// own docstring says "the mature fix is eslint's react-hooks/rules-of-hooks
// ... it should be deleted the day eslint lands". This is that day.
//
// The bug it replaces: the S2D.4 honesty pass added an early return to eleven
// surfaces ("this read failed — here is why"). In two of them a `useMemo` sat
// BELOW the insertion point, so on a failed read React rendered fewer hooks
// than the previous render and crashed. Neither `tsc` nor the unit tests can
// see it — the types are fine and the crash only fires when that surface is
// open AND its read fails.
//
// The python tripwire reasoned about indentation, not scope, and only about
// `export function` components. `rules-of-hooks` reasons about the actual
// control-flow graph, so it also sees hooks in arrow-function components, in
// loops, in `&&` short-circuits, and after `try`/`catch` — none of which the
// tripwire could. Both directions of the swap are negative-controlled in the
// PR that lands this.

import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";

export default tseslint.config(
  {
    // Build output, dependencies, and the Rust target dirs are not ours to
    // lint. `target/` at the repo root holds tauri-codegen's asset blobs —
    // minified and sometimes binary — which parse as neither JS nor anything
    // else. Cargo puts a target dir in both places depending on how the build
    // was invoked, so both are named.
    ignores: [
      "dist/**",
      "node_modules/**",
      "target/**",
      "src-tauri/target/**",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  // v7 keeps the eslintrc-shaped configs at the top level and the flat-config
  // ones under `.flat`; the top-level one fails at load with "plugins must be
  // an object".
  reactHooks.configs.flat["recommended-latest"],
  {
    files: ["**/*.{ts,tsx}"],
    rules: {
      // The two rules this config exists for. `rules-of-hooks` is the
      // replacement tripwire and is a hard error — a violation is a runtime
      // crash on the failure path, not a style opinion.
      "react-hooks/rules-of-hooks": "error",
      // `exhaustive-deps` catches the OTHER half of the S4 bug class: the
      // Dashboard read its counts once at mount and never again, so the
      // packaged app sat showing "Governed 0" with a decision already on the
      // chain. Stale in a way that reads exactly like wrong.
      "react-hooks/exhaustive-deps": "error",

      // An unused variable in a governance surface is usually a wiring mistake
      // — a value read from the bridge and then not rendered. Underscore is the
      // escape hatch for a deliberately-ignored binding.
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  {
    // Test files may use `any` where they are deliberately constructing an
    // invalid shape to prove the code under test rejects it.
    files: ["**/*.test.{ts,tsx}"],
    rules: { "@typescript-eslint/no-explicit-any": "off" },
  },
);
