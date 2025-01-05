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
    label       text    not null,
    permissions text    not null,
    type        integer not null default 0 check ( type in (0, 1) ), -- this could be FK to another table
    foreign key (user_id) references users (user_id),
    constraint unique_label_per_user unique (user_id, label)
);

