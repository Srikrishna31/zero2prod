# zero2prod
This repository follows the book: [Zero to Production in Rust](https://www.zero2prod.com/index.html) and the corresponding [Github Repo](https://github.com/LukeMathWalker/zero-to-production)

For the PostGRESql Database setup, following links were helpful in configuring on Windows and Linux:
1. [https://stackoverflow.com/questions/4465475/where-is-the-postgresql-config-file-postgresql-conf-on-windows]()
2. [https://stackoverflow.com/questions/55038942/fatal-password-authentication-failed-for-user-postgres-postgresql-11-with-pg]()
3. [https://stackoverflow.com/questions/37694987/connecting-to-postgresql-in-a-docker-container-from-outside]()
4. [https://stackoverflow.com/questions/25540711/docker-postgres-pgadmin-local-connection]()

# Dev Environment Pre-reqs
The code itself doesn't have any platform dependency. However, the Database init script is written in shell script, so a Linux environment is assumed. To ensure it to run on Windows, [Cygwin](https://www.cygwin.com/) or [Msys2](https://www.msys2.org/) is required.
