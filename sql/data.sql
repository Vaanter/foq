INSERT INTO users(username, password)
VALUES ('testuser1', '$argon2id$v=19$m=19456,t=2,p=1$xS9QG9glzsQ9R7Er/L/zQw$kFDa3+IQ+baHI445Vs5RRdFEHf9g4KU09r5HYMfX+ZM');

INSERT INTO views(user_id, root, label, permissions)
VALUES (last_insert_rowid(), 'C:\', 'c', 'r;l;w;c') ON CONFLICT DO NOTHING;

INSERT INTO users(username, password)
VALUES ('testuser2', '$argon2id$v=19$m=19456,t=2,p=1$2oBXOgFkwft9WAyunU1/eA$tLgFjcfaQ3WBxhybAkQTEdVRafgLJTsl3JzY2gUqi5A');

INSERT INTO users(username, password)
VALUES ('testuser3', '$argon2id$v=19$m=19456,t=2,p=1$wdd9R3bV4juf5+zBb3qmig$TAMrnpTWqd62b0f0Wp8tSIvpCWSQI2x0OW/8yPd/KGg');

INSERT INTO views(user_id, root, label, permissions)
VALUES (last_insert_rowid(), 'ROOT', 'LABEL', 'INVALID');