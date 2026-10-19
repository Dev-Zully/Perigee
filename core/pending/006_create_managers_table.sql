-- Migration 005: Manager onboarding with approval/KYC gate (API-33).
-- Anyone can register as a manager, but they are `pending` until an
-- operator approves them. Only approved managers may create vaults.

CREATE TABLE IF NOT EXISTS managers (
    id TEXT PRIMARY KEY,
    stellar_address TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    email TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'approved', 'rejected')),
    kyc_document_ref TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_managers_status ON managers(status);
CREATE INDEX IF NOT EXISTS idx_managers_stellar_address ON managers(stellar_address);
