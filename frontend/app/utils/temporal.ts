export interface TemporalType {
  kind: 'date' | 'time' | 'datetime'
  zoned: boolean
}
export interface TemporalParts {
  year: number
  month: number
  day: number
  hour: number
  minute: number
  second: number
  fraction: string
  offset: string
  separator: 'T' | ' '
  seconds: boolean
}
export function temporalType(dataType: string, provider = 'postgres'): TemporalType | null {
  const type = dataType
    .toLowerCase()
    .replace(/\(\d+\)/g, '')
    .replace(/\s+/g, ' ')
    .trim()
  if (type === 'date') return { kind: 'date', zoned: false }
  if (/^(?:time(?: with(?:out)? time zone)?|timetz)$/.test(type))
    return { kind: 'time', zoned: type === 'timetz' || type === 'time with time zone' }
  if (provider === 'sqlserver' && type === 'timestamp') return null // rowversion, not a date
  if (
    /^(?:timestamp(?: with(?:out)? time zone)?|timestamptz|datetime(?:2|offset)?|smalldatetime)$/.test(
      type,
    )
  )
    return { kind: 'datetime', zoned: /with time zone|timestamptz|datetimeoffset/.test(type) }
  return null
}
export function daysInMonth(year: number, month: number) {
  if (
    !Number.isInteger(year) ||
    year < 1 ||
    year > 9999 ||
    !Number.isInteger(month) ||
    month < 1 ||
    month > 12
  )
    return 0
  return [
    31,
    year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0) ? 29 : 28,
    31,
    30,
    31,
    30,
    31,
    31,
    30,
    31,
    30,
    31,
  ][month - 1]!
}
export function utcCalendarDate(year: number, month: number, day: number) {
  const value = new Date(0)
  value.setUTCFullYear(year, month - 1, day)
  value.setUTCHours(0, 0, 0, 0)
  return value
}
export function validTemporal(parts: TemporalParts, type: TemporalType) {
  if (
    type.kind !== 'time' &&
    (!daysInMonth(parts.year, parts.month) ||
      !Number.isInteger(parts.day) ||
      parts.day < 1 ||
      parts.day > daysInMonth(parts.year, parts.month))
  )
    return false
  if (
    type.kind !== 'date' &&
    (![parts.hour, parts.minute, parts.second].every(Number.isInteger) ||
      parts.hour < 0 ||
      parts.hour > 23 ||
      parts.minute < 0 ||
      parts.minute > 59 ||
      parts.second < 0 ||
      parts.second > 59 ||
      !/^\d{0,9}$/.test(parts.fraction))
  )
    return false
  if (parts.offset && !/^(?:Z|[+-](?:[01]\d|2[0-3])(?::?[0-5]\d)?)$/.test(parts.offset))
    return false
  return !type.zoned || !!parts.offset
}
export function parseTemporal(value: string, type: TemporalType): TemporalParts | null {
  const date = type.kind !== 'time'
  const match = date
    ? value.match(
        /^(\d{4})-(\d{2})-(\d{2})(?:([ T])(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,9}))?)?(?:\s*(Z|[+-]\d{2}(?::?\d{2})?))?)?$/,
      )
    : value.match(/^(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,9}))?)?(?:\s*(Z|[+-]\d{2}(?::?\d{2})?))?$/)
  if (!match || (type.kind === 'date' && match[4]) || (type.kind === 'datetime' && !match[4]))
    return null
  const parts: TemporalParts = date
    ? {
        year: Number(match[1]),
        month: Number(match[2]),
        day: Number(match[3]),
        separator: match[4] === ' ' ? ' ' : 'T',
        hour: Number(match[5] ?? 0),
        minute: Number(match[6] ?? 0),
        second: Number(match[7] ?? 0),
        fraction: match[8] ?? '',
        offset: match[9] ?? '',
        seconds: match[7] !== undefined,
      }
    : {
        year: 2000,
        month: 1,
        day: 1,
        separator: 'T',
        hour: Number(match[1]),
        minute: Number(match[2]),
        second: Number(match[3] ?? 0),
        fraction: match[4] ?? '',
        offset: match[5] ?? '',
        seconds: match[3] !== undefined,
      }
  return validTemporal(parts, type) ? parts : null
}
export function formatTemporal(parts: TemporalParts, type: TemporalType) {
  const pad = (value: number) => String(value).padStart(2, '0')
  const date = `${String(parts.year).padStart(4, '0')}-${pad(parts.month)}-${pad(parts.day)}`
  const seconds =
    parts.seconds || parts.second !== 0 || parts.fraction
      ? `:${pad(parts.second)}${parts.fraction ? '.' + parts.fraction : ''}`
      : ''
  const time = `${pad(parts.hour)}:${pad(parts.minute)}${seconds}${parts.offset}`
  return type.kind === 'date'
    ? date
    : type.kind === 'time'
      ? time
      : `${date}${parts.separator}${time}`
}
