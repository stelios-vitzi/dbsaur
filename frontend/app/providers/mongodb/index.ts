import { PostgreSQL } from '@codemirror/lang-sql'
export default {
  editing: false,
  nameLimit: 128,
  recordHelp: 'MongoDB documents use canonical Extended JSON to preserve BSON types.',
  ddlWarning: 'Experimental MongoDB connections are read only.',
  port: 27017,
  username: '',
  maintenance: 'through the admin database',
  dialect: PostgreSQL,
}
