use exocore_protos::store::{
    boolean_predicate, entity_query::Predicate, ordering, trait_field_predicate, trait_query,
    EntityQuery, MatchPredicate, Ordering, Paging, TraitFieldPredicate,
    TraitFieldReferencePredicate,
};
use tantivy::{
    query::{AllQuery, BooleanQuery, FuzzyTermQuery, Occur, PhraseQuery, Query, TermQuery},
    schema::{Field, IndexRecordOption},
    Index, Term,
};

use super::{schema::Fields, MutationIndexConfig};
use crate::error::Error;

pub(crate) struct ParsedQuery {
    pub tantivy: Box<dyn Query>,
    pub paging: Paging,
    pub ordering: Ordering,
    pub trait_name: Option<String>,
}

pub(crate) struct QueryParser<'i, 'f, 'q> {
    index: &'i Index,
    fields: &'f Fields,
    config: &'f MutationIndexConfig,
    proto: &'q EntityQuery,
    query: Option<Box<dyn Query>>,
    paging: Paging,
    ordering: Ordering,
    trait_name: Option<String>,
}

impl<'i, 'f, 'q> QueryParser<'i, 'f, 'q> {
    pub fn parse(
        index: &'i Index,
        fields: &'f Fields,
        config: &'f MutationIndexConfig,
        proto: &'q EntityQuery,
    ) -> Result<ParsedQuery, Error> {
        let mut parser = QueryParser {
            index,
            fields,
            config,
            proto,
            query: None,
            paging: Default::default(),
            ordering: Default::default(),
            trait_name: None,
        };

        parser.inner_parse()?;

        let tantivy_query = parser
            .query
            .as_ref()
            .map(|q| q.box_clone())
            .ok_or_else(|| Error::QueryParsing(anyhow!("query didn't didn't get parsed")))?;

        Ok(ParsedQuery {
            tantivy: tantivy_query,
            paging: parser.paging,
            ordering: parser.ordering,
            trait_name: parser.trait_name,
        })
    }

    fn inner_parse(&mut self) -> Result<(), Error> {
        let predicate = self
            .proto
            .predicate
            .as_ref()
            .ok_or(Error::ProtoFieldExpected("predicate"))?;

        self.paging = self.proto.paging.clone().unwrap_or(Paging {
            after_ordering_value: None,
            before_ordering_value: None,
            count: self.config.iterator_page_size,
            offset: 0,
        });
        self.ordering = self.proto.ordering.clone().unwrap_or_default();
        self.query = Some(self.parse_predicate(predicate)?);

        Ok(())
    }

    fn parse_predicate(&mut self, predicate: &Predicate) -> Result<Box<dyn Query>, Error> {
        match predicate {
            Predicate::Match(match_pred) => self.parse_match_pred(match_pred),
            Predicate::Trait(trait_pred) => self.parse_trait_pred(trait_pred),
            Predicate::Ids(ids_pred) => self.parse_ids_pred(ids_pred),
            Predicate::Reference(ref_pred) => self.parse_ref_pred(self.fields.all_refs, ref_pred),
            Predicate::Operations(op_pred) => self.parse_operation_pred(op_pred),
            Predicate::All(all_pred) => self.parse_all_pred(all_pred),
            Predicate::Boolean(bool_pred) => self.parse_bool_pred(bool_pred),
            Predicate::Test(_) => Err(anyhow!("Query failed for tests").into()),
        }
    }

    fn parse_match_pred(&mut self, match_pred: &MatchPredicate) -> Result<Box<dyn Query>, Error> {
        if self.ordering.value.is_none() {
            self.ordering.value = Some(ordering::Value::Score(true));
        }

        let field = self.fields.all_text;
        let text = match_pred.query.as_str();
        let no_fuzzy = match_pred.no_fuzzy;
        Ok(Box::new(self.new_fuzzy_query(field, text, no_fuzzy)?))
    }

    fn parse_trait_pred(
        &mut self,
        trait_pred: &exocore_protos::store::TraitPredicate,
    ) -> Result<Box<dyn Query>, Error> {
        if self.ordering.value.is_none() {
            self.ordering.value = Some(ordering::Value::OperationId(true));
        }

        let mut queries: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        let trait_type = Term::from_field_text(self.fields.trait_type, &trait_pred.trait_name);
        let trait_type_query = TermQuery::new(trait_type, IndexRecordOption::Basic);
        queries.push((Occur::Must, Box::new(trait_type_query)));

        if let Some(cur_trait_name) = self.trait_name.as_ref() {
            if cur_trait_name != &trait_pred.trait_name {
                return Err(Error::QueryParsing(anyhow!(
                    "can't query multiple traits: current={} new={}",
                    cur_trait_name,
                    trait_pred.trait_name
                )));
            }
        } else {
            self.trait_name = Some(trait_pred.trait_name.clone());
        }

        if let Some(trait_query) = &trait_pred.query {
            match &trait_query.predicate {
                Some(trait_query::Predicate::Match(trait_pred)) => {
                    queries.push((Occur::Must, self.parse_match_pred(trait_pred)?));
                }
                Some(trait_query::Predicate::Field(field_trait_pred)) => {
                    queries.push((
                        Occur::Must,
                        self.parse_trait_field_predicate(field_trait_pred)?,
                    ));
                }
                Some(trait_query::Predicate::Reference(field_ref_pred)) => {
                    queries.push((
                        Occur::Must,
                        self.parse_trait_field_reference_predicate(field_ref_pred)?,
                    ));
                }
                None => {}
            }
        }

        Ok(Box::new(BooleanQuery::from(queries)))
    }

    fn parse_trait_field_predicate(
        &self,
        predicate: &TraitFieldPredicate,
    ) -> Result<Box<dyn Query>, Error> {
        use exocore_protos::reflect::FieldType as FT;
        use trait_field_predicate::Value as PV;

        let trait_name = self
            .trait_name
            .as_ref()
            .ok_or_else(|| Error::QueryParsing(anyhow!("expected trait name")))?;

        let fields = self
            .fields
            .get_dynamic_trait_field_prefix(trait_name, &predicate.field)?;

        let mut queries: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        for field in fields {
            match (&field.field_type, &predicate.value) {
                (FT::String, Some(PV::String(value))) => {
                    let term = Term::from_field_text(field.field, value);

                    queries.push((Occur::Should, Box::new(TermQuery::new(term, IndexRecordOption::Basic))));
                }
                (ft, pv) => {
                    return Err(
                        Error::QueryParsing(
                            anyhow!(
                                "Incompatible field type vs field value in predicate: trait_name={} field={}, field_type={:?}, value={:?}",
                                trait_name,
                                predicate.field,
                                ft,
                                pv,
                            ))
                    )
                }
            }
        }

        Ok(Box::new(BooleanQuery::from(queries)))
    }

    fn parse_trait_field_reference_predicate(
        &mut self,
        predicate: &TraitFieldReferencePredicate,
    ) -> Result<Box<dyn Query>, Error> {
        let trait_name = self
            .trait_name
            .as_ref()
            .ok_or_else(|| Error::QueryParsing(anyhow!("expected trait name")))?;

        let field = self
            .fields
            .get_dynamic_trait_field(trait_name, &predicate.field)?;

        let reference = predicate
            .reference
            .as_ref()
            .ok_or(Error::ProtoFieldExpected("reference"))?;

        self.parse_ref_pred(field.field, reference)
    }

    fn parse_ids_pred(
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

    fn parse_ref_pred(
        &mut self,
        field: Field,
        ref_pred: &exocore_protos::store::ReferencePredicate,
    ) -> Result<Box<dyn Query>, Error> {
        if self.ordering.value.is_none() {
            self.ordering.value = Some(ordering::Value::OperationId(true));
        }

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

    fn parse_operation_pred(
        &mut self,
        op_pred: &exocore_protos::store::OperationsPredicate,
    ) -> Result<Box<dyn Query>, Error> {
        if self.ordering.value.is_none() {
            self.ordering.value = Some(ordering::Value::OperationId(true));
            self.ordering.ascending = true;
        }

        let mut queries: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        for operation_id in &op_pred.operation_ids {
            let op_term = Term::from_field_u64(self.fields.operation_id, *operation_id);
            let op_query = TermQuery::new(op_term, IndexRecordOption::Basic);
            queries.push((Occur::Should, Box::new(op_query)));
        }
        Ok(Box::new(BooleanQuery::from(queries)))
    }

    fn parse_all_pred(
        &mut self,
        _all_pred: &exocore_protos::store::AllPredicate,
    ) -> Result<Box<dyn Query>, Error> {
        if self.ordering.value.is_none() {
            self.ordering.value = Some(ordering::Value::OperationId(true));
            self.ordering.ascending = false;
        }

        Ok(Box::new(AllQuery))
    }

    fn parse_bool_pred(
        &mut self,
        bool_pred: &exocore_protos::store::BooleanPredicate,
    ) -> Result<Box<dyn Query>, Error> {
        use boolean_predicate::{sub_query::Predicate as SubPredicate, Occur as ProtoOccur};
        let mut queries: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        for sub_query in &bool_pred.queries {
            let predicate = sub_query
                .predicate
                .as_ref()
                .ok_or(Error::ProtoFieldExpected("predicate"))?;

            let tantivy_query = match predicate {
                SubPredicate::Match(match_pred) => self.parse_match_pred(match_pred)?,
                SubPredicate::Trait(trait_pred) => self.parse_trait_pred(trait_pred)?,
                SubPredicate::Ids(ids_pred) => self.parse_ids_pred(ids_pred)?,
                SubPredicate::Reference(ref_pred) => {
                    self.parse_ref_pred(self.fields.all_refs, ref_pred)?
                }
                SubPredicate::Operations(op_pred) => self.parse_operation_pred(op_pred)?,
                SubPredicate::All(all_pred) => self.parse_all_pred(all_pred)?,
                SubPredicate::Boolean(bool_pred) => self.parse_bool_pred(bool_pred)?,
            };

            let tantivy_occur = match ProtoOccur::from_i32(sub_query.occur) {
                Some(ProtoOccur::Should) => Occur::Should,
                Some(ProtoOccur::Must) => Occur::Must,
                Some(ProtoOccur::MustNot) => Occur::MustNot,
                None => {
                    return Err(Error::QueryParsing(anyhow!(
                        "Invalid occur value: {}",
                        sub_query.occur
                    )));
                }
            };

            queries.push((tantivy_occur, tantivy_query));
        }

        Ok(Box::new(BooleanQuery::from(queries)))
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
}
