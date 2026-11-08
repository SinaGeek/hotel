import js from '@eslint/js'
import reactHooks from 'eslint-plugin-react-hooks'
import tseslint from 'typescript-eslint'

export default tseslint.config(
  { ignores: ['dist', 'src/types/generated'] },
  {
    extends: [js.configs.recommended, tseslint.configs.recommended, reactHooks.configs['recommended-latest']],
    files: ['**/*.{ts,tsx}'],
  },
)
