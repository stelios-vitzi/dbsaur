// Disposable CI fixture only. No existing database or certificate is consulted.
import { execFileSync } from 'node:child_process'
import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs'

const container = process.env.MYSQL_TEST_CONTAINER
if (!container) throw new Error('MYSQL_TEST_CONTAINER must name the disposable test container')
function exec(...args) {
  return execFileSync('docker', ['exec', '--user', 'mysql', container, ...args], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  })
}
exec(
  'openssl',
  'req',
  '-x509',
  '-newkey',
  'rsa:2048',
  '-nodes',
  '-keyout',
  '/tmp/dbsaur-test-ca-key.pem',
  '-out',
  '/tmp/dbsaur-test-ca.pem',
  '-days',
  '1',
  '-subj',
  '/CN=DBsaur Disposable Test CA',
)
exec(
  'openssl',
  'req',
  '-new',
  '-newkey',
  'rsa:2048',
  '-nodes',
  '-keyout',
  '/tmp/dbsaur-test-key.pem',
  '-out',
  '/tmp/dbsaur-test.csr',
  '-subj',
  '/CN=localhost',
  '-addext',
  'subjectAltName=IP:127.0.0.1,DNS:localhost',
  '-addext',
  'basicConstraints=critical,CA:FALSE',
)
exec(
  'openssl',
  'x509',
  '-req',
  '-in',
  '/tmp/dbsaur-test.csr',
  '-CA',
  '/tmp/dbsaur-test-ca.pem',
  '-CAkey',
  '/tmp/dbsaur-test-ca-key.pem',
  '-CAcreateserial',
  '-out',
  '/tmp/dbsaur-test-cert.pem',
  '-days',
  '1',
  '-copy_extensions',
  'copy',
)
exec(
  'mysql',
  '-uroot',
  '-pdisposable_test_only',
  '-e',
  "SET GLOBAL ssl_ca='/tmp/dbsaur-test-ca.pem'; SET GLOBAL ssl_cert='/tmp/dbsaur-test-cert.pem'; SET GLOBAL ssl_key='/tmp/dbsaur-test-key.pem'; ALTER INSTANCE RELOAD TLS;",
)
const connection = JSON.parse(process.env.WEBDB_TEST_MYSQL_CONNECTION_JSON)
connection.ssl = true
connection.tlsCa = exec('cat', '/tmp/dbsaur-test-ca.pem')
const json = JSON.stringify(connection)
if (process.env.GITHUB_ENV)
  appendFileSync(process.env.GITHUB_ENV, `WEBDB_TEST_MYSQL_CONNECTION_JSON=${json}\n`)
else {
  mkdirSync('.artifacts', { recursive: true })
  writeFileSync('.artifacts/mysql-test-connection.json', json)
}
