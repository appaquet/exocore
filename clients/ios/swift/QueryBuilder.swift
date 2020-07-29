import Foundation
import SwiftProtobuf

public class QueryBuilder {
    private var inner: Exocore_Index_EntityQuery

    init() {
        self.inner = Exocore_Index_EntityQuery()
    }

    public static func withTrait<M: Message>(_ message: M.Type) -> QueryBuilder {
        let builder = QueryBuilder()
        var traitPredicate = Exocore_Index_TraitPredicate()
        traitPredicate.traitName = M.protoMessageName
        builder.inner.trait = traitPredicate
        return builder
    }

    public static func withTrait<M: Message>(_ message: M.Type, query: Exocore_Index_TraitQuery) -> QueryBuilder {
        let builder = QueryBuilder()
        var traitPredicate = Exocore_Index_TraitPredicate()
        traitPredicate.traitName = M.protoMessageName
        traitPredicate.query = query
        builder.inner.trait = traitPredicate
        return builder
    }

    public static func matching(query: String) -> QueryBuilder {
        let builder = QueryBuilder()
        var queryPredicate = Exocore_Index_MatchPredicate()
        queryPredicate.query = query
        builder.inner.match = queryPredicate
        return builder
    }

    public func count(_ count: Int) -> QueryBuilder {
        var paging = Exocore_Index_Paging()
        paging.count = UInt32(count)
        self.inner.paging = paging

        return self
    }

    public func orderByField(_ field: String, ascending: Bool = false) -> QueryBuilder {
        var ordering = Exocore_Index_Ordering()
        ordering.field = field
        ordering.ascending = ascending
        self.inner.ordering = ordering

        return self
    }

    public func build() -> Exocore_Index_EntityQuery {
        self.inner
    }
}

public class TraitQueryBuilder {
    private var inner: Exocore_Index_TraitQuery

    init() {
        self.inner = Exocore_Index_TraitQuery()
    }

    public static func refersTo(field: String, entityId: String, traitId: String = "") -> TraitQueryBuilder {
        let builder = TraitQueryBuilder()
        var predicate = Exocore_Index_TraitFieldReferencePredicate()
        predicate.field = field
        predicate.reference = Exocore_Index_ReferencePredicate()
        predicate.reference.entityID = entityId
        predicate.reference.traitID = traitId
        builder.inner.reference = predicate

        return builder
    }

    public func build() -> Exocore_Index_TraitQuery {
        self.inner
    }
}
