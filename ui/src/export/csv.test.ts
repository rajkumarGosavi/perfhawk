import { describe, expect, it } from 'vitest'
import { toCsv } from './csv'

describe('toCsv', () => {
  it('quotes per RFC 4180 and uses CRLF', () => {
    expect(toCsv(['name', 'n'], [['a,"b"', 3], ['x\ny', 1.5], ['plain', 0]])).toBe(
      'name,n\r\n"a,""b""",3\r\n"x\ny",1.5\r\nplain,0\r\n',
    )
  })
  it('empty body gives only the header row', () => {
    expect(toCsv(['a', 'b'], [])).toBe('a,b\r\n')
  })
})
