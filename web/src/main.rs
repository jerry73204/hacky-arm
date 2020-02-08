#![feature(proc_macro_hygiene)]
#![feature(decl_macro)]

#[macro_use]
extern crate rocket;

mod routes;

use failure::Fallible;

fn main() -> Fallible<()> {
    rocket::ignite()
        .mount("/", routes![routes::index_page])
        .launch();
    Ok(())
}
