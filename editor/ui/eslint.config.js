import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "node_modules", "src/api/generated", "test-results", "playwright-report"] },
  js.configs.recommended,
  ...tseslint.configs.strict,
  { plugins: { "react-hooks": reactHooks }, rules: { ...reactHooks.configs.recommended.rules } },
  // Playwright fixtures: `use` is Playwright's callback (not a React hook) and `{}` is required.
  { files: ["tests/e2e/**/*.ts"], rules: { "react-hooks/rules-of-hooks": "off", "no-empty-pattern": "off" } },
  // Build scripts run under Node.
  { files: ["scripts/**/*.mjs"], languageOptions: { globals: { process: "readonly", console: "readonly" } } },
);
