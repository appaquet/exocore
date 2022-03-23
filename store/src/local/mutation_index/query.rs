use exocore_protos::store::{
    entity_query::Predicate, ordering, EntityQuery, MatchPredicate, Ordering, Paging,
};
use tantivy::{
    query::{AllQuery, BooleanQuery, FuzzyTermQuery, Occur, PhraseQuery, Query, TermQuery},
    schema::{Field, IndexRecordOption},
    Index, Term,
};

use crate::error::Error;

use super::schema::Fields;

pub(crate) struct ParsedQuery<'i, 'f, 'q> {
    index: &'i Index,
    fields: &'f Fields,
    proto: &'q EntityQuery,
    paging: Paging,
    ordering: Ordering,
    trait_name: Option<String>,
    query: Option<Box<dyn Query>>,
}

impl<'i, 'f, 'q> ParsedQuery<'i, 'f, 'q> {
    pub fn new(
        index: &'i Index,
        fields: &'f Fields,
        proto: &'q EntityQuery,
    ) -> Result<ParsedQuery<'i, 'f, 'q>, Error> {
        let mut query = ParsedQuery {
            index,
            fields,
            proto,
            paging: Default::default(),
            ordering: Default::default(),
            trait_name: None,
            query: None,
        };

        query.parse()?;

        Ok(query)
    }

    fn parse(&mut self) -> Result<(), Error> {
        let predicate = self
            .proto
            .predicate
            .as_ref()
            .ok_or(Error::ProtoFieldExpected("predicate"))?;

        self.paging = self.proto.paging.clone().unwrap_or_default();
        self.ordering = self.proto.ordering.clone().unwrap_or_default();
        self.query = Some(self.parse_predicate(predicate)?);

        Ok(())
    }

    fn parse_predicate(&mut self, predicate: &Predicate) -> Result<Box<dyn Query>, Error> {
        match predicate {
            Predicate::Match(match_pred) => self.parse_match_predicate(match_pred),
            Predicate::Trait(trait_pred) => self.parse_trait_predicate(trait_pred),
            Predicate::Ids(ids_pred) => self.parse_ids_predicate(ids_pred),
            Predicate::Reference(ref_pred) => {
                self.parse_reference_predicate(self.fields.all_refs, ref_pred)
            }
            Predicate::Operations(op_pred) => self.parse_operation_predicate(op_pred),
            Predicate::All(all_pred) => self.parse_all_predicate(all_pred),
            Predicate::Boolean(_) => todo!(),
            Predicate::Test(_) => Err(anyhow!("Query failed for tests").into()),
        }
    }

    fn parse_match_predicate(
        &mut self,
        match_pred: &MatchPredicate,
    ) -> Result<Box<dyn Query>, Error> {
        if self.ordering.value.is_none() {
            self.ordering.value = Some(ordering::Value::Score(true));
        }

        let field = self.fields.all_text;
        let text = match_pred.query.as_str();
        let no_fuzzy = match_pred.no_fuzzy;
        Ok(Box::new(self.new_fuzzy_query(field, text, no_fuzzy)?))
    }

    fn parse_operation_predicate(
        &mut self,
        op_pred: &exocore_protos::store::OperationsPredicate,
    ) -> Result<Box<dyn Query>, Error> {
        let mut queries: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        for operation_id in &op_pred.operation_ids {
            let op_term = Term::from_field_u64(self.fields.operation_id, *operation_id);
            let op_query = TermQuery::new(op_term, IndexRecordOption::Basic);
            queries.push((Occur::Should, Box::new(op_query)));
        }
        let query = BooleanQuery::from(queries);

        if self.ordering.value.is_none() {
            self.ordering.value = Some(ordering::Value::OperationId(true));
            self.ordering.ascending = true;
        }

        Ok(Box::new(query))
    }

    fn parse_ids_predicate(
        &mut self,
        ids_pred: &exocore_protos::store::IdsPredicate,
    ) -> Result<Box<dyn Query>, Error> {
        let mut queries: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        for entity_id in &ids_pred.ids {
            let term = Term::from_field_text(self.fields.entity_id, entity_id);
            let query = TermQuery::new(term, IndexRecordOption::Basic);
            queries.push((Occur::Should, Box::new(query)));
        }
        let query = BooleanQuery::from(queries);

        if self.ordering.value.is_none() {
            self.ordering.value = Some(ordering::Value::OperationId(true));
        }

        Ok(Box::new(query))
    }

    fn parse_all_predicate(
        &mut self,
        _all_pred: &exocore_protos::store::AllPredicate,
    ) -> Result<Box<dyn Query>, Error> {
        if self.ordering.value.is_none() {
            self.ordering.value = Some(ordering::Value::OperationId(true));
            self.ordering.ascending = false;
        }

        Ok(Box::new(AllQuery))
    }

    fn parse_reference_predicate(
        &mut self,
        field: Field,
        ref_pred: &exocore_protos::store::ReferencePredicate,
    ) -> Result<Box<dyn Query>, Error> {
        let query: Box<dyn tantivy::query::Query> = if !ref_pred.trait_id.is_empty() {
            let terms = vec![
                Term::from_field_text(field, &format!("entity{}", ref_pred.entity_id)),
                Term::from_field_text(field, &format!("trait{}", ref_pred.trait_id)),
            ];
            Box::new(PhraseQuery::new(terms))
        } else {
            Box::new(TermQuery::new(
                Term::from_field_text(field, &format!("entity{}", ref_pred.entity_id)),
                IndexRecordOption::Basic,
            ))
        };

        Ok(query)
    }

    fn parse_trait_predicate(
        &mut self,
        trait_pred: &exocore_protos::store::TraitPredicate,
    ) -> Result<Box<dyn Query>, Error> {
        todo!()
    }

    fn new_fuzzy_query(
        &self,
        field: Field,
        text: &str,
        no_fuzzy: bool,
    ) -> Result<BooleanQuery, Error> {
        let tok = self.index.tokenizer_for_field(field)?;
        let mut queries: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        let mut stream = tok.token_stream(text);

        while stream.advance() {
            let token = stream.token().text.as_str();
            let term = Term::from_field_text(field, token);

            if !no_fuzzy && token.len() > 3 {
                let max_distance = if token.len() > 6 { 2 } else { 1 };
                let query = Box::new(FuzzyTermQuery::new(term.clone(), max_distance, true));
                queries.push((Occur::Should, query));
            }

            // even if fuzzy is enabled, we add the term again so that an exact match scores
            // more
            let query = Box::new(TermQuery::new(
                term,
                IndexRecordOption::WithFreqsAndPositions,
            ));
            queries.push((Occur::Should, query));
        }

        Ok(BooleanQuery::from(queries))
    }

    pub fn to_tantivy(&self) -> Box<dyn tantivy::query::Query> {
        unimplemented!()
    }
}
