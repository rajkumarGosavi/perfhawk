import { describe, expect, it } from 'vitest'
import { buildCliCommand, validateRecordForm } from './recordLogic'

const ok = { pid: '123', url: '', durationSecs: '30' }

describe('validateRecordForm', () => {
  it('accepts a pid or a loopback url', () => {
    expect(validateRecordForm(ok)).toBeNull()
    expect(validateRecordForm({ pid: '', url: 'http://127.0.0.1:6060', durationSecs: '30' })).toBeNull()
    expect(validateRecordForm({ pid: '', url: 'https://localhost/x', durationSecs: '1' })).toBeNull()
    expect(validateRecordForm({ pid: '', url: 'http://[::1]:6060', durationSecs: '600' })).toBeNull()
  })

  it('needs exactly one target', () => {
    expect(validateRecordForm({ ...ok, pid: '' })).toMatch(/pid or a url/i)
    expect(validateRecordForm({ ...ok, url: 'http://localhost:1' })).toMatch(/only one/i)
  })

  it('rejects a bad pid', () => {
    for (const pid of ['0', '-4', 'abc', '1.5', '12 3']) {
      expect(validateRecordForm({ ...ok, pid }), pid).not.toBeNull()
    }
  })

  it('rejects urls that are not local http(s)', () => {
    for (const url of [
      'http://evil.com',
      'http://127.0.0.1.evil.com',
      'file:///etc/passwd',
      'http://localhost@evil.com/',
      'ftp://localhost',
      'nonsense',
    ]) {
      expect(validateRecordForm({ pid: '', url, durationSecs: '30' }), url).toMatch(/local/i)
    }
  })

  it('keeps duration within 1 to 600 seconds', () => {
    for (const durationSecs of ['0', '601', '', 'x', '2.5', '-1']) {
      expect(validateRecordForm({ ...ok, durationSecs }), durationSecs).toMatch(/duration/i)
    }
    expect(validateRecordForm({ ...ok, durationSecs: '1' })).toBeNull()
    expect(validateRecordForm({ ...ok, durationSecs: '600' })).toBeNull()
  })
})

describe('buildCliCommand', () => {
  it('writes the equivalent terminal command', () => {
    expect(buildCliCommand({ pid: '123', url: '', durationSecs: '30', kinds: [] })).toBe(
      'perfhawk record --pid 123 --duration 30s --open',
    )
    expect(
      buildCliCommand({ pid: '', url: 'http://127.0.0.1:6060', durationSecs: '5', kinds: ['cpu', 'wall'] }),
    ).toBe('perfhawk record --url http://127.0.0.1:6060 --kinds cpu,wall --duration 5s --open')
  })
})
