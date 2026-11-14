/// <reference types="node" />
import { readFileSync } from 'node:fs'

/** The stylesheets (tokens, base, components) as one text, for tests (vitest returns an empty string for `.css?raw`). */
export const themeCss: string = ['tokens', 'base', 'components']
  .map((f) => readFileSync(new URL(`./styles/${f}.css`, import.meta.url), 'utf8').replace(/\r\n/g, '\n'))
  .join('\n')
