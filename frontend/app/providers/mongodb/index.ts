import { PostgreSQL } from '@codemirror/lang-sql'
export default {
  editing: true,
  nameLimit: 128,
  recordHelp: 'MongoDB documents use canonical Extended JSON to preserve BSON types.',
  ddlWarning: 'Document changes require confirmation.',
  port: 27017,
  username: '',
  maintenance: 'through the admin database',
  dialect: PostgreSQL,
}
