import sqlite from './sqlite/index.ts'
import postgres from './postgres/index.ts'
import mysql from './mysql/index.ts'
import mariadb from './mariadb/index.ts'
import sqlserver from './sqlserver/index.ts'

export function providerOptions(provider: string) {
  return (
    ({ postgres, mysql, mariadb, sqlserver, sqlite } as Record<string, typeof postgres>)[
      provider
    ] ?? postgres
  )
}
export function quoteIdentifier(name: string, provider = 'postgres') {
  if (provider === 'mysql' || provider === 'mariadb') return '`' + name.replaceAll('`', '``') + '`'
  if (provider === 'sqlserver') return '[' + name.replaceAll(']', ']]') + ']'
  return '"' + name.replaceAll('"', '""') + '"'
}
