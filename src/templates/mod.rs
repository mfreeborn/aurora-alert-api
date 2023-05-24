use derive_more::Display;
pub use tera::{Context, Tera};

pub type TemplateEngine = Tera;

pub fn init() -> Result<TemplateEngine, tera::Error> {
    // Instantiating Tera this way ensures that we can guarantee all the templates
    // are loaded succesfully as soon as the app starts
    let engine = Tera::new("./src/templates/*.html").expect("failed to load templates");

    Ok(engine)
}

#[derive(Debug, Display)]
pub enum Template {
    #[display(fmt = "alert.html")]
    Alert,
}

impl Template {
    pub fn render(
        &self,
        context: &Context,
        template_engine: &TemplateEngine,
    ) -> Result<String, anyhow::Error> {
        let x = Ok(template_engine.render(&self.to_string(), context)?);
        dbg!(&x);
        x
    }
}
