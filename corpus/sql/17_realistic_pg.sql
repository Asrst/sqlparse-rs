-- schema dump
CREATE TABLE users (
  id SERIAL PRIMARY KEY,
  name TEXT NOT NULL,
  age INTEGER DEFAULT 0 CHECK (age >= 0),
  created_at TIMESTAMP
);
INSERT INTO users (name, age) VALUES ('alice', 30), ('bob', NULL);
CREATE FUNCTION add_one(x int) RETURNS int AS $$
BEGIN
  RETURN x + 1;
END;
$$ LANGUAGE plpgsql;
SELECT u.name, count(*) AS n FROM users u GROUP BY u.name ORDER BY n DESC;
