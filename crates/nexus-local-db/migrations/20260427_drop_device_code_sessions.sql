-- Drop the device_code_sessions table — removed as part of V1.10 daemon cleanup.
-- The mock device flow handlers were deleted; this table is no longer needed.
DROP TABLE IF EXISTS device_code_sessions;
