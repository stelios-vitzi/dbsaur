export const privacyConfig = {
  email: 'steliosvitzi@gmail.com',
  updated: '11 November 2026',
  version: 3,
  storageKey: 'webdb.privacy-notice.v1',
  retentionDays: 180,
}

export function validPrivacyChoice(raw: string | null, now = Date.now()): boolean {
  try {
    const value = JSON.parse(raw || 'null')
    return (
      value?.version === privacyConfig.version &&
      typeof value.expires === 'number' &&
      value.expires > now &&
      value.expires <= now + privacyConfig.retentionDays * 86400000
    )
  } catch {
    return false
  }
}

export const legalDocuments = {
  privacy: {
    title: 'Privacy policy',
    introduction: 'How DBsaur handles your connections, queries, and database data.',
    sections: [
      [
        'Who operates DBsaur',
        'DBsaur is an open-source desktop application running on your computer. You control the application data directory and your selected database connections. Project maintainers do not receive your database content through the software. Standalone web hosting is not supported.',
      ],
      [
        'Database connections and content',
        'Connection details, SQL queries, database metadata, and records pass between the bundled interface, the local Rust process, and the database you select. The application API is restricted to loopback and a private desktop session. Connect only to databases you are authorized to use. The database owner remains responsible for the lawful use of personal data held in that database.',
      ],
      [
        'Sessions and saved connections',
        'Active connections and credentials are held in local process memory for up to eight hours. Disconnecting removes the active session; closing the app stops its local API. An HttpOnly, SameSite=Strict cookie named webdb_session identifies the database session, and dbsaur_desktop protects the desktop API. Saved connection details are encrypted in connections.sqlite using vault.key in your per-user application data directory until removed. Backups can retain copies. Credentials are never persisted in the interface local storage.',
      ],
      [
        'Browser storage and downloads',
        'Queries you explicitly save remain in the embedded webview localStorage under webdb.saved-queries.v1 until removed or the app data is cleared. Avoid saving secrets in SQL. The privacy-notice preference stores only its version and expiry for 180 days under webdb.privacy-notice.v1. Unsaved drafts and displayed records remain in memory. JSON exports and clipboard copies remain under your control outside DBsaur. The webview may cache bundled application assets.',
      ],
      [
        'Purposes and legal grounds',
        'Connection and query data are processed to perform the database operations you request. You or your organization remain responsible for the lawful processing of personal data in selected databases. This software notice does not certify your use as compliant with data-protection law.',
      ],
      [
        'Recipients and international transfers',
        'Database requests go from your computer to your selected database server over your configured network connection. Signed release builds contact GitHub to check for updates and download them; GitHub receives connection metadata such as your IP address, but no database credentials, queries, or records. DBsaur includes no analytics, advertising, tracking pixels, or remote font services. Your database and network providers control their own locations and data handling.',
      ],
      [
        'Map backgrounds',
        'PostGIS geometry previews are processed by your selected database and displayed in the desktop application. Opening a map loads vector tiles and fonts from vijimaps.com, sharing your IP address and the viewed map area with that provider. You can turn off VijiTiles background in the preview. Geometry values, queries, and record attributes are not included in tile requests.',
      ],
      [
        'Your GDPR rights',
        'Applicable data-protection rights depend on your jurisdiction and the data being processed. Contact the organization responsible for your database for requests concerning its records; do not send credentials or database records to project maintainers. DBsaur includes no automated decision-making or profiling feature.',
      ],
      [
        'Control and contact',
        'Remove saved queries in the query library, remove saved connections on the connection screen, disconnect sessions, and clear your notice preference through Privacy settings. For records held by your database, contact its operator. For questions about the software itself, write to steliosvitzi@gmail.com.',
      ],
    ],
  },
  terms: {
    title: 'Terms of use',
    introduction: 'The conditions for using DBsaur to access and manage databases.',
    sections: [
      [
        'Operator and scope',
        'DBsaur is a desktop application for a trusted, single-user workstation. Its local API is embedded in the native process and protected by a private desktop session. It does not provide a hosted web service or multi-user access.',
      ],
      [
        'Open-source license',
        'DBsaur source code is distributed under the MIT license, including its warranty disclaimer and limitation of liability. These usage notes do not restrict the rights granted by that license. Third-party dependencies retain their own licenses. The full license and notices are included in the project repository.',
      ],
      [
        'Authorized use',
        'Use DBsaur only with databases, accounts, and records you are authorized to access. You are responsible for the legality of your queries, the data you process, and any permissions required from database owners or individuals. Do not attempt to bypass security controls, disrupt services, access another user’s information, or use the tool unlawfully.',
      ],
      [
        'Database changes and backups',
        'Browsing and query execution are intended for reading data. Separate editing and settings actions can insert, update, or delete records and alter database configuration or structure. Review the target database, values, and confirmation before applying a change. Keep appropriate backups and use database accounts with only the permissions you need. Applied changes may be irreversible.',
      ],
      [
        'Credentials and shared devices',
        'Protect your database credentials and use trusted devices and secure connections. Saved connections belong to the encrypted store in your OS user data directory. Saved queries reside in the embedded webview storage and may be available to others using the same OS account. Disconnect and remove saved information when appropriate.',
      ],
      [
        'Availability and accuracy',
        'DBsaur is provided as is, without warranty, as set out in the MIT license. Features, compatibility, availability, and displayed results may change or contain errors. Verify important results independently. There is no guaranteed support, maintenance schedule, security certification, or fitness for a particular purpose.',
      ],
      [
        'Responsibility and statutory rights',
        'You remain responsible for the database operations you request. To the extent permitted by law, the operator is not responsible for losses caused by unauthorized use, incorrect instructions, third-party systems, or failure to maintain backups. Nothing in these terms excludes liability that cannot legally be excluded or limits mandatory consumer or data-protection rights.',
      ],
      [
        'Privacy, third parties, and changes',
        'The Privacy policy explains how the application handles data. Third-party database, update-hosting, map, and network services are subject to their own terms. These notes may change as the software changes; the date above identifies the current version.',
      ],
    ],
  },
} as const
