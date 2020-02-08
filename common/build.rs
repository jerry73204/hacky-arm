use failure::Fallible;
use std::path::PathBuf;

fn main() -> Fallible<()> {
    // generate types from protobuf files
    {
        let protobuf_dir = PathBuf::from("protobuf");
        let proto_paths = glob::glob(protobuf_dir.join("*.proto").to_str().unwrap())?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        prost_build::compile_protos(&proto_paths, &[protobuf_dir])?;
    }

    Ok(())
}
