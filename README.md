# music\_quiz

This is a simple music quiz. 

# Usage

To run, build (as this uses sqlx, you'll need to provide a `DATABASE_URL`) and then execute with two arguments:

* the first argument should be a path to a config file that looks like this:

    ``` toml
    database_url = "postgres://localhost:12345/music_quiz"
    cache_duration = "1d"
    bind_address = "0.0.0.0:8080"
    ```

* the second argument should be either `migrate` or `run`. `migrate` will run the migration scripts on the database at `database_url`, while `run` will run the server. have fun.

also this only works with postgres

also also the executable will need to be in the same directory as `static`

# License

This work is licensed under GPLv3, except the fonts, which are licensed under their respective licenses.
