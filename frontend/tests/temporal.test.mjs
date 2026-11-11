import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  daysInMonth,
  formatTemporal,
  parseTemporal,
  temporalType,
  utcCalendarDate,
} from '../app/utils/temporal.ts'

test('temporal types distinguish SQL Server rowversion and preserve provider date types', () => {
  assert.equal(temporalType('timestamp', 'sqlserver'), null)
  for (const type of [
    'date',
    'datetime(6)',
    'timestamp without time zone',
    'timestamptz',
    'datetimeoffset(7)',
    'time(6) with time zone',
  ])
    assert.ok(temporalType(type), type)
  assert.equal(temporalType('integer'), null)
})
test('dates and times round-trip without changing fractions or offsets', () => {
  for (const [type, value] of [
    ['date', '2024-02-29'],
    ['timestamp with time zone', '2026-11-12T23:59:59.123456+02:30'],
    ['datetime(6)', '2026-11-12 03:04:05.000001'],
    ['datetimeoffset(7)', '2026-11-12T03:04:05.1234567-05:00'],
    ['time with time zone', '03:04:05.123456+00'],
    ['time', '03:04'],
  ]) {
    const kind = temporalType(type)
    const parsed = parseTemporal(value, kind)
    assert.ok(parsed, value)
    assert.equal(formatTemporal(parsed, kind), value)
  }
})
test('calendar changes keep the original clock precision and timezone', () => {
  const kind = temporalType('timestamptz')
  const parsed = parseTemporal('2026-11-12T23:59:59.123456+02:30', kind)
  assert.equal(formatTemporal({ ...parsed, day: 13 }, kind), '2026-11-13T23:59:59.123456+02:30')
  assert.equal(daysInMonth(2000, 2), 29)
  assert.equal(daysInMonth(1900, 2), 28)
  assert.equal(utcCalendarDate(12, 1, 1).getUTCFullYear(), 12)
})
test('invalid dates, missing zoned offsets, and special values remain text-only', () => {
  for (const value of ['2025-02-29', '2026-13-01', 'infinity', '-infinity'])
    assert.equal(parseTemporal(value, temporalType('date')), null)
  assert.equal(parseTemporal('2026-11-12T24:00:00Z', temporalType('timestamptz')), null)
  assert.equal(parseTemporal('2026-11-12T10:00:00', temporalType('timestamptz')), null)
})
