-- The functions that PUBLIC runs by default and that the probe refuses (R7, finding H1): the statement of 8a.13
-- for these five names. Run by roles.sql after \connect, in each database except probe_public.
SELECT format('REVOKE EXECUTE ON FUNCTION %s FROM PUBLIC', p.oid::regprocedure)
FROM pg_proc p
WHERE p.pronamespace = 'pg_catalog'::regnamespace
  AND p.proname IN ('pg_logical_emit_message', 'pg_terminate_backend', 'pg_cancel_backend', 'ts_stat', 'ts_rewrite')
\gexec
