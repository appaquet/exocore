pub trait Query {
    fn fields(&self) -> Vec<&str>;
    fn value(&self) -> &str;
}

pub struct FieldMatchQuery<'a> {
    pub field_name: &'a str,
    pub value: &'a str,
}

impl<'a> Query for FieldMatchQuery<'a> {
    fn fields(&self) -> Vec<&str> {
        vec![self.field_name]
    }

    fn value(&self) -> &str {
        self.value
    }
}
