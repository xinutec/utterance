// @ts-check
// ESLint flat config for the Angular frontend. Type-aware: typescript-eslint
// recommendedTypeChecked + stylisticTypeChecked (parserOptions.projectService)
// for usage bugs tsc and syntactic lint miss (floating/misused promises, unsafe
// `any`, await-thenable), plus the Angular rules — external templates only, and
// template accessibility.

import angular from "angular-eslint";
import tseslint from "typescript-eslint";

export default tseslint.config(
  // ts-rs writes src/app/generated/ from the Rust types — don't lint generated code.
  { ignores: ["src/app/generated/**"] },
  {
    files: ["src/**/*.ts"],
    extends: [
      ...tseslint.configs.recommendedTypeChecked,
      ...tseslint.configs.stylisticTypeChecked,
      ...angular.configs.tsRecommended,
    ],
    languageOptions: {
      parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname },
    },
    processor: angular.processInlineTemplates,
    rules: {
      "@angular-eslint/component-max-inline-declarations": ["error", { template: 0, styles: 0 }],
      "@typescript-eslint/no-empty-function": "off",
    },
  },
  {
    // Tests legitimately use `any` for mocks, DOM stubs and fixtures — relax the
    // unsafe-any family here; app code stays fully type-checked.
    files: ["src/**/*.spec.ts"],
    rules: {
      "@typescript-eslint/no-unsafe-member-access": "off",
      "@typescript-eslint/no-unsafe-call": "off",
      "@typescript-eslint/no-unsafe-assignment": "off",
      "@typescript-eslint/no-unsafe-argument": "off",
      "@typescript-eslint/no-unsafe-return": "off",
    },
  },
  {
    files: ["src/**/*.html"],
    extends: [...angular.configs.templateRecommended, ...angular.configs.templateAccessibility],
  },
);
