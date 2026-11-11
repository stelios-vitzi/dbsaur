export class DatabaseChangeCancelled extends Error {
  constructor() {
    super('Database change cancelled.')
    this.name = 'DatabaseChangeCancelled'
  }
}
