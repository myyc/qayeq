fn main() {
    // Compile GResources
    glib_build_tools::compile_resources(
        &["resources"],
        "resources/qayeq.gresource.xml",
        "qayeq.gresource",
    );
}
