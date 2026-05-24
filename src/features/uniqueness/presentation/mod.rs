// Presentation layer for uniqueness (thin for now).
// No public HTTP routes are exposed yet; uniqueness enforcement is
// performed internally during registration and via admin delete flows.
// Future work could add admin endpoints to inspect or release specific
// device locks.

// Intentionally empty — re-export nothing until admin uniqueness routes
// are required. The module exists to satisfy the clean-architecture
// convention used by inventory/locations/campaigns.
