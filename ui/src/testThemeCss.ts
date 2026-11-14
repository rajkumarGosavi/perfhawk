/// <reference types="node" />
import { readFileSync } from 'node:fs'

/** theme.css as text, for tests (vitest returns an empty string for `.css?raw`). */
export const themeCss: string = readFileSync(new URL('./theme.css', import.meta.url), 'utf8').replace(/\r\n/g, '\n')
