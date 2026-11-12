import { SQLite } from '@codemirror/lang-sql'
export default {
  editing: true,
  nameLimit: 128,
  recordHelp:
    'SQLite applies column affinity to text input. Binary fields use hexadecimal text. Updates and deletes require a non-NULL primary key.',
  ddlWarning:
    'Changes commit after confirmation. SQLite key and type changes may require rebuilding a table. Database files are managed outside DBsaur.',
  port: 0,
  username: '',
  maintenance: 'to the selected file',
  dialect: SQLite,
}
