#include <cstdlib>
#include <fstream>
#include <map>
#include <memory>
#include <set>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

#include "calendar.h"
#include "cata_catch.h"
#include "item.h"
#include "item_group.h"
#include "json.h"
#include "rng.h"
#include "type_id.h"

namespace
{

constexpr const char *baseline_commit = "4dfd36038b16650dc1b5cb9d79a3e42363174b05";
constexpr const char *upstream_tree = "210f31db2e8b2f0caed1809f1a66781859f9d129";
constexpr unsigned int maximum_seed_search = 100000;

class trace_spawn final : public Item_spawn_data
{
    public:
        trace_spawn( std::string label, int probability, std::vector<std::string> &trace )
            : Item_spawn_data( probability, "Rust item-group oracle trace leaf" )
            , label_( std::move( label ) )
            , trace_( trace )
        {
        }

        std::size_t create( ItemList &, const time_point &, RecursionList &,
                            spawn_flags ) const override
        {
            trace_.push_back( label_ );
            return 0;
        }

        item create_single( const time_point &birthday, RecursionList & ) const override
        {
            trace_.push_back( label_ );
            return item( itype_id::NULL_ID(), birthday );
        }

        void check_consistency( bool ) const override
        {
        }

        bool remove_item( const itype_id & ) override
        {
            return false;
        }

        void replace_items( const std::unordered_map<itype_id, itype_id> & ) override
        {
        }

        bool has_item( const itype_id & ) const override
        {
            return false;
        }

        std::set<const itype *> every_item() const override
        {
            return {};
        }

        std::map<const itype *, std::pair<int, int>> every_item_min_max() const override
        {
            return {};
        }

    private:
        const std::string label_;
        std::vector<std::string> &trace_;
};

unsigned int seed_for_first_draw( int low, int high, int target )
{
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        rng_set_engine_seed( seed );
        if( rng( low, high ) == target ) {
            return seed;
        }
    }
    FAIL( "could not find a bounded seed for requested item-group oracle draw" );
    return 1;
}

unsigned int seed_for_collection_branch( int probability )
{
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        rng_set_engine_seed( seed );
        static_cast<void>( rng( 0, 99 ) );
        if( rng( 0, 99 ) < probability ) {
            return seed;
        }
    }
    FAIL( "could not find a bounded seed for collection branch" );
    return 1;
}

std::vector<std::string> collection_trace( unsigned int seed, int probability,
        int &downstream_draw )
{
    std::vector<std::string> trace;
    Item_group group( Item_group::G_COLLECTION, 100, 0, 0, "Rust item-group collection oracle" );
    group.add_entry( std::make_unique<trace_spawn>( "first", 100, trace ) );
    group.add_entry( std::make_unique<trace_spawn>( "conditional", probability, trace ) );
    group.add_entry( std::make_unique<trace_spawn>( "last", 100, trace ) );
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( seed );
    group.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    downstream_draw = rng( 0, 99 );
    return trace;
}

std::vector<std::string> distribution_trace( unsigned int seed )
{
    std::vector<std::string> trace;
    Item_group group( Item_group::G_DISTRIBUTION, 100, 0, 0,
                      "Rust item-group distribution oracle" );
    group.add_entry( std::make_unique<trace_spawn>( "low", 2, trace ) );
    group.add_entry( std::make_unique<trace_spawn>( "middle", 3, trace ) );
    group.add_entry( std::make_unique<trace_spawn>( "high", 5, trace ) );
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( seed );
    group.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    return trace;
}

std::vector<std::string> nested_trace( unsigned int seed, int probability,
                                      int &downstream_draw )
{
    std::vector<std::string> trace;
    auto child = std::make_unique<Item_group>( Item_group::G_COLLECTION, 100, 0, 0,
                 "Rust item-group nested child oracle" );
    child->add_entry( std::make_unique<trace_spawn>( "child_conditional", probability, trace ) );
    child->add_entry( std::make_unique<trace_spawn>( "child_always", 100, trace ) );

    Item_group root( Item_group::G_COLLECTION, 100, 0, 0,
                     "Rust item-group nested root oracle" );
    root.add_entry( std::move( child ) );
    root.add_entry( std::make_unique<trace_spawn>( "root_last", 100, trace ) );
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( seed );
    root.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    downstream_draw = rng( 0, 99 );
    return trace;
}

std::size_t create_with_count( unsigned int seed, int minimum, int maximum )
{
    Single_item_creator creator( "test_pipe", Single_item_creator::S_ITEM, 100,
                                 "Rust item-group count oracle" );
    creator.modifier.emplace();
    creator.modifier->count = { minimum, maximum };
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( seed );
    creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    return items.size();
}

int create_with_charges( unsigned int seed, int minimum, int maximum )
{
    Single_item_creator creator( "40x46mm_m1006", Single_item_creator::S_ITEM, 100,
                                 "Rust item-group charges oracle" );
    creator.modifier.emplace();
    creator.modifier->charges = { minimum, maximum };
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( seed );
    creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    REQUIRE( items.size() == 1 );
    REQUIRE( items.front().count_by_charges() );
    return items.front().charges;
}

unsigned int seed_for_observed_charges( int minimum, int maximum, int target )
{
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        if( create_with_charges( seed, minimum, maximum ) == target ) {
            return seed;
        }
    }
    FAIL( "could not find a bounded seed for requested charges result" );
    return 1;
}

void write_trace( JsonOut &json, const std::vector<std::string> &trace )
{
    json.start_array();
    for( const std::string &label : trace ) {
        json.write( label );
    }
    json.end_array();
}

} // namespace

TEST_CASE( "rust_cpp_oracle_item_group_generation", "[cpp-oracle][item-group]" )
{
    const char *output_path = std::getenv( "CDDA_RUST_CPP_ORACLE_OUTPUT" );
    REQUIRE( output_path != nullptr );
    REQUIRE( std::string( output_path ).size() > 0 );
    REQUIRE( itype_id( "test_pipe" ).is_valid() );
    REQUIRE( itype_id( "40x46mm_m1006" ).is_valid() );

    constexpr int branch_probability = 50;
    const unsigned int collection_seed = seed_for_collection_branch( branch_probability );
    rng_set_engine_seed( collection_seed );
    static_cast<void>( rng( 0, 99 ) );
    const bool collection_conditional = rng( 0, 99 ) < branch_probability;
    static_cast<void>( rng( 0, 99 ) );
    const int collection_expected_downstream = rng( 0, 99 );
    std::vector<std::string> collection_expected{ "first" };
    if( collection_conditional ) {
        collection_expected.emplace_back( "conditional" );
    }
    collection_expected.emplace_back( "last" );
    int collection_actual_downstream = -1;
    const std::vector<std::string> collection_actual = collection_trace(
                collection_seed, branch_probability, collection_actual_downstream );

    constexpr int distribution_tickets[] = { 1, 2, 3, 5, 6, 10 };
    std::vector<std::pair<int, std::string>> distribution_results;
    for( const int ticket : distribution_tickets ) {
        const unsigned int seed = seed_for_first_draw( 1, 10, ticket );
        const std::vector<std::string> trace = distribution_trace( seed );
        REQUIRE( trace.size() == 1 );
        distribution_results.emplace_back( ticket, trace.front() );
    }

    const unsigned int count_minimum_seed = seed_for_first_draw( 2, 4, 2 );
    const unsigned int count_maximum_seed = seed_for_first_draw( 2, 4, 4 );
    const std::size_t fixed_count = create_with_count( 1, 3, 3 );
    const std::size_t minimum_count = create_with_count( count_minimum_seed, 2, 4 );
    const std::size_t maximum_count = create_with_count( count_maximum_seed, 2, 4 );

    const int fixed_charges = create_with_charges( 1, 4, 4 );
    const int zero_charges = create_with_charges( 1, 0, 0 );
    const unsigned int minimum_charges_seed = seed_for_observed_charges( 1, 4, 1 );
    const unsigned int maximum_charges_seed = seed_for_observed_charges( 1, 4, 4 );
    const int minimum_charges = create_with_charges( minimum_charges_seed, 1, 4 );
    const int maximum_charges = create_with_charges( maximum_charges_seed, 1, 4 );

    const unsigned int nested_seed = seed_for_collection_branch( branch_probability );
    rng_set_engine_seed( nested_seed );
    static_cast<void>( rng( 0, 99 ) );
    const bool nested_conditional = rng( 0, 99 ) < branch_probability;
    static_cast<void>( rng( 0, 99 ) );
    static_cast<void>( rng( 0, 99 ) );
    const int nested_expected_downstream = rng( 0, 99 );
    std::vector<std::string> nested_expected;
    if( nested_conditional ) {
        nested_expected.emplace_back( "child_conditional" );
    }
    nested_expected.emplace_back( "child_always" );
    nested_expected.emplace_back( "root_last" );
    int nested_actual_downstream = -1;
    const std::vector<std::string> nested_actual = nested_trace(
                nested_seed, branch_probability, nested_actual_downstream );

    std::ofstream output( output_path, std::ios::out | std::ios::trunc );
    REQUIRE( output.is_open() );
    {
        JsonOut json( output );
        json.start_object();
        json.member( "format_version", 1 );
        json.member( "baseline_commit", baseline_commit );
        json.member( "upstream_tree", upstream_tree );
        json.member( "kernel", "item_group_generation_v1" );

        json.member( "collection" );
        json.start_object();
        json.member( "entry_probability", branch_probability );
        json.member( "rolls_consumed", 3 );
        json.member( "expected_trace" );
        write_trace( json, collection_expected );
        json.member( "actual_trace" );
        write_trace( json, collection_actual );
        json.member( "downstream_draw_matches",
                     collection_expected_downstream == collection_actual_downstream );
        json.end_object();

        json.member( "distribution" );
        json.start_array();
        for( const auto &[ticket, selected] : distribution_results ) {
            json.start_object();
            json.member( "ticket", ticket );
            json.member( "selected", selected );
            json.end_object();
        }
        json.end_array();

        json.member( "counts" );
        json.start_array();
        json.start_object();
        json.member( "case_id", "fixed" );
        json.member( "minimum", 3 );
        json.member( "maximum", 3 );
        json.member( "target", 3 );
        json.member( "observed", static_cast<int>( fixed_count ) );
        json.end_object();
        json.start_object();
        json.member( "case_id", "range_minimum" );
        json.member( "minimum", 2 );
        json.member( "maximum", 4 );
        json.member( "target", 2 );
        json.member( "observed", static_cast<int>( minimum_count ) );
        json.end_object();
        json.start_object();
        json.member( "case_id", "range_maximum" );
        json.member( "minimum", 2 );
        json.member( "maximum", 4 );
        json.member( "target", 4 );
        json.member( "observed", static_cast<int>( maximum_count ) );
        json.end_object();
        json.end_array();

        json.member( "charges" );
        json.start_array();
        json.start_object();
        json.member( "case_id", "fixed" );
        json.member( "minimum", 4 );
        json.member( "maximum", 4 );
        json.member( "target", 4 );
        json.member( "observed", fixed_charges );
        json.end_object();
        json.start_object();
        json.member( "case_id", "zero_clamped_to_one" );
        json.member( "minimum", 0 );
        json.member( "maximum", 0 );
        json.member( "target", 1 );
        json.member( "observed", zero_charges );
        json.end_object();
        json.start_object();
        json.member( "case_id", "range_minimum" );
        json.member( "minimum", 1 );
        json.member( "maximum", 4 );
        json.member( "target", 1 );
        json.member( "observed", minimum_charges );
        json.end_object();
        json.start_object();
        json.member( "case_id", "range_maximum" );
        json.member( "minimum", 1 );
        json.member( "maximum", 4 );
        json.member( "target", 4 );
        json.member( "observed", maximum_charges );
        json.end_object();
        json.end_array();

        json.member( "nested" );
        json.start_object();
        json.member( "rolls_consumed", 4 );
        json.member( "expected_trace" );
        write_trace( json, nested_expected );
        json.member( "actual_trace" );
        write_trace( json, nested_actual );
        json.member( "downstream_draw_matches",
                     nested_expected_downstream == nested_actual_downstream );
        json.end_object();
        json.end_object();
    }
    output.close();
    REQUIRE( output.good() );
}
