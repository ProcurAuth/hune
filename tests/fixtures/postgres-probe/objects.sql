-- The objects of each probe database. Run by roles.sql after \connect.
CREATE SCHEMA metrics;
CREATE TABLE metrics.t (id int, v text);
CREATE VIEW metrics.v AS SELECT id FROM metrics.t;
CREATE SEQUENCE metrics.seq;
GRANT USAGE ON SCHEMA metrics TO probe_readers;
GRANT SELECT ON ALL TABLES IN SCHEMA metrics TO probe_readers;
-- A table that nobody can read: the test of 42501.
CREATE TABLE metrics.hidden (x int);
