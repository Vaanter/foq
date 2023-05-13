create table if not exists users
(
    user_id  integer primary key autoincrement not null,
    username text                              not null,
    password text                              not null
);

create table if not exists views
(
    user_id     integer not null,
    root        text    not null,
    label       text    not null unique,
    permissions text    not null,
    foreign key (user_id) references users (user_id)
);

