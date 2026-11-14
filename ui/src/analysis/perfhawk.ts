/**
 * Hide Perfhawk's own frames. The Go agent runs inside the profiled process, so its
 * work (the agent, its fgprof wall-clock sampler, and the pprof decoder it uses) shows
 * up in the profile. This regex goes to the server's existing `exclude` filter, which
 * drops every sample whose stack has a matching frame. (`hide` would instead fold the
 * cost into the caller and mislabel it.) The .NET helper runs out of process, so
 * nothing needs excluding there.
 */
export const PERFHAWK_FRAMES = String.raw`rajkumarGosavi/perfhawk/agents/go|felixge/fgprof|google/pprof/profile\.`

/** The `exclude` regex to send: the user's own regex, plus Perfhawk's frames when `on`. */
export function effectiveExclude(userExclude: string | undefined, on: boolean | undefined): string | undefined {
  if (!on) return userExclude
  return userExclude ? `(?:${userExclude})|(?:${PERFHAWK_FRAMES})` : PERFHAWK_FRAMES
}
