#include <algorithm>
#include <ctime>
#include <cstdlib>
#include <fstream>
#include <limits>
#include <list>
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
#include "itype.h"
#include "json.h"
#include "options.h"
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
        trace_spawn( std::string label, int probability, std::vector<std::string> &trace,
                     holiday event = holiday::none )
            : Item_spawn_data( probability, "Rust item-group oracle trace leaf", event )
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

std::vector<std::string> event_distribution_trace( unsigned int seed )
{
    std::vector<std::string> trace;
    Item_group group( Item_group::G_DISTRIBUTION, 100, 0, 0,
                      "Rust item-group event distribution oracle" );
    group.add_entry( std::make_unique<trace_spawn>(
                         "inactive_event", 3, trace, holiday::christmas ) );
    group.add_entry( std::make_unique<trace_spawn>( "ordinary", 2, trace ) );
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

int downstream_after_fixed_count( unsigned int seed )
{
    Single_item_creator creator( "test_pipe", Single_item_creator::S_ITEM, 100,
                                 "Rust item-group modifier RNG oracle" );
    creator.modifier.emplace();
    creator.modifier->count = { 1, 1 };
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( seed );
    creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    REQUIRE( items.size() == 1 );
    return rng( 0, 9999 );
}

struct constructor_variant_trace {
    unsigned int seed = 0;
    std::string selected;
    std::string name;
    std::string description;
    int downstream_draw = 0;
};

std::vector<constructor_variant_trace> observe_constructor_variants()
{
    std::map<std::string, constructor_variant_trace> first_by_variant;
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        rng_set_engine_seed( seed );
        const item generated( itype_id( "test_rock" ) );
        REQUIRE( generated.has_itype_variant() );
        const std::string selected = generated.itype_variant().id;
        first_by_variant.try_emplace( selected, constructor_variant_trace{
            seed, selected, generated.tname( 1, false ), generated.variant_description(),
            rng( 0, 9999 )
        } );
        if( first_by_variant.size() == 2 ) {
            break;
        }
    }
    REQUIRE( first_by_variant.count( "test_rock_blue" ) == 1 );
    REQUIRE( first_by_variant.count( "test_rock_green" ) == 1 );
    REQUIRE( first_by_variant.at( "test_rock_blue" ).name == "blue test_rock" );
    REQUIRE( first_by_variant.at( "test_rock_green" ).name == "green test_rock" );
    REQUIRE( first_by_variant.at( "test_rock_blue" ).description ==
             "A rock the size of a baseball.  Makes a decent melee weapon, and is also good for throwing at enemies.  It's a blue test rock" );
    REQUIRE( first_by_variant.at( "test_rock_green" ).description ==
             "A rock the size of a baseball.  Makes a decent melee weapon, and is also good for throwing at enemies.  It's a green test rock" );
    std::vector<constructor_variant_trace> traces;
    for( const std::string &variant : { "test_rock_blue", "test_rock_green" } ) {
        traces.push_back( first_by_variant.at( variant ) );
    }
    return traces;
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

void write_strings( JsonOut &json, const std::set<std::string> &values )
{
    json.start_array();
    for( const std::string &value : values ) {
        json.write( value );
    }
    json.end_array();
}

class scoped_option_override
{
    public:
        scoped_option_override( const std::string &option, const std::string &value ) :
            option_( option ), old_value_( get_options().get_option( option ).getValue( true ) )
        {
            get_options().get_option( option_ ).setValue( value );
        }

        scoped_option_override( const scoped_option_override & ) = delete;
        scoped_option_override &operator=( const scoped_option_override & ) = delete;

        ~scoped_option_override()
        {
            get_options().get_option( option_ ).setValue( old_value_ );
        }

    private:
        std::string option_;
        std::string old_value_;
};

class scoped_holiday_override
{
    public:
        explicit scoped_holiday_override( std::time_t time )
            : holiday_( get_holiday_from_time( time, true ) )
        {
        }

        scoped_holiday_override( const scoped_holiday_override & ) = delete;
        scoped_holiday_override &operator=( const scoped_holiday_override & ) = delete;

        ~scoped_holiday_override()
        {
            static_cast<void>( get_holiday_from_time( 0, true ) );
        }

        holiday value() const
        {
            return holiday_;
        }

    private:
        holiday holiday_;
};

struct container_observation {
    std::string case_id;
    int seed_search_limit = 0;
    bool valid_shapes = true;
    int minimum_top_level = std::numeric_limits<int>::max();
    int maximum_top_level = 0;
    int minimum_contents = std::numeric_limits<int>::max();
    int maximum_contents = 0;
    std::set<std::string> content_orders;
    std::set<std::string> outside_types;
    struct exact_trace {
        std::string witness;
        unsigned int seed = 0;
        std::vector<std::string> top_level_types;
        std::vector<std::string> content_types;
    };
    std::vector<exact_trace> exact_traces;
};

container_observation observe_container_group( const std::string &case_id,
        const item_group_id &group_id )
{
    container_observation observation;
    observation.case_id = case_id;
    observation.seed_search_limit = maximum_seed_search;
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        rng_set_engine_seed( static_cast<unsigned int>( seed ) );
        const item_group::ItemList items = item_group::items_from( group_id );
        observation.minimum_top_level = std::min( observation.minimum_top_level,
                                        static_cast<int>( items.size() ) );
        observation.maximum_top_level = std::max( observation.maximum_top_level,
                                        static_cast<int>( items.size() ) );
        const item *container = nullptr;
        std::vector<std::string> top_level_types;
        for( const item &candidate : items ) {
            top_level_types.push_back( candidate.typeId().str() );
            if( candidate.typeId() == itype_id( "test_balloon" ) ) {
                if( container != nullptr ) {
                    observation.valid_shapes = false;
                    break;
                }
                container = &candidate;
            } else {
                observation.outside_types.insert( candidate.typeId().str() );
            }
        }
        if( !observation.valid_shapes || container == nullptr ) {
            observation.valid_shapes = false;
            break;
        }
        const std::list<const item *> contents = container->all_items_top();
        observation.minimum_contents = std::min( observation.minimum_contents,
                                       static_cast<int>( contents.size() ) );
        observation.maximum_contents = std::max( observation.maximum_contents,
                                       static_cast<int>( contents.size() ) );
        std::string order;
        std::vector<std::string> content_types;
        for( const item *content : contents ) {
            if( !order.empty() ) {
                order += ",";
            }
            order += content->typeId().str();
            content_types.push_back( content->typeId().str() );
        }
        if( observation.content_orders.insert( order ).second ) {
            observation.exact_traces.push_back( {
                "first_content_order:" + order, seed, top_level_types, content_types
            } );
        }
        const bool outside_complete = case_id == "discard" || observation.outside_types.size() == 3;
        if( observation.content_orders.size() == 6 && outside_complete ) {
            break;
        }
    }
    return observation;
}

struct corpse_observation {
    int seed_search_limit = 0;
    bool valid_shapes = true;
    std::set<std::string> wrapper_types;
    std::set<int> wrapper_raw_damage;
    std::set<int> wrapper_damage_levels;
    std::set<int> content_counts;
    bool observed_pristine_content = false;
    bool observed_damage_four_content = false;
    struct exact_trace {
        std::string witness;
        unsigned int seed = 0;
        std::string wrapper_type;
        int wrapper_raw_damage = 0;
        int wrapper_damage_level = 0;
        std::vector<std::string> content_types;
        std::vector<int> content_raw_damage;
        std::vector<int> content_damage_levels;
    };
    std::vector<exact_trace> exact_traces;
};

corpse_observation observe_everyday_corpses()
{
    corpse_observation observation;
    observation.seed_search_limit = maximum_seed_search;
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        rng_set_engine_seed( static_cast<unsigned int>( seed ) );
        const item_group::ItemList items = item_group::items_from( item_group_id( "everyday_corpse" ) );
        if( items.size() != 1 ) {
            observation.valid_shapes = false;
            break;
        }
        const item &corpse = items.front();
        observation.wrapper_types.insert( corpse.typeId().str() );
        observation.wrapper_raw_damage.insert( corpse.damage() );
        observation.wrapper_damage_levels.insert( corpse.damage_level() );
        const std::list<const item *> contents = corpse.all_items_top();
        if( contents.empty() ) {
            observation.valid_shapes = false;
            break;
        }
        observation.content_counts.insert( static_cast<int>( contents.size() ) );
        std::vector<std::string> content_types;
        std::vector<int> content_raw_damage;
        std::vector<int> content_damage_levels;
        bool has_pristine_content = false;
        bool has_damage_four_content = false;
        for( const item *content : contents ) {
            content_types.push_back( content->typeId().str() );
            content_raw_damage.push_back( content->damage() );
            content_damage_levels.push_back( content->damage_level() );
            has_pristine_content = has_pristine_content ||
                                   ( content->damage() == 0 && content->damage_level() == 0 );
            has_damage_four_content = has_damage_four_content ||
                                      ( content->damage() == 4 * itype::damage_scale &&
                                        content->damage_level() == 5 );
        }
        const bool first_damage_four = has_damage_four_content &&
                                       !observation.observed_damage_four_content;
        observation.observed_pristine_content = observation.observed_pristine_content ||
                                                has_pristine_content;
        observation.observed_damage_four_content = observation.observed_damage_four_content ||
                                                   has_damage_four_content;
        const auto retain_trace = [&]( const std::string &witness ) {
            observation.exact_traces.push_back( {
                witness,
                seed,
                corpse.typeId().str(),
                corpse.damage(),
                corpse.damage_level(),
                content_types,
                content_raw_damage,
                content_damage_levels
            } );
        };
        if( seed == 1 ) {
            retain_trace( "fixed_seed:1" );
        }
        if( first_damage_four ) {
            retain_trace( "first_damage_four_content" );
        }
        if( observation.wrapper_types.size() == 3 && observation.wrapper_raw_damage.size() == 1 &&
            observation.wrapper_damage_levels.size() == 1 && observation.content_counts.size() > 1 &&
            observation.observed_pristine_content && observation.observed_damage_four_content ) {
            break;
        }
    }
    return observation;
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

    constexpr unsigned int modifier_rng_seed = 73;
    rng_set_engine_seed( modifier_rng_seed );
    static_cast<void>( rng( 0, std::numeric_limits<int>::max() ) ); // item seed
    static_cast<void>( rng_bits() ); // empty itype-variant selection still draws
    static_cast<void>( rng( 0, 2 ) ); // unconditional one_in( 3 ) fit roll
    static_cast<void>( rng( 0, 0 ) ); // default Item_modifier damage roll
    const int modifier_rng_expected_downstream = rng( 0, 9999 );
    const int modifier_rng_actual_downstream = downstream_after_fixed_count( modifier_rng_seed );
    const std::vector<constructor_variant_trace> constructor_variants =
        observe_constructor_variants();

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

    Item_modifier damaged;
    damaged.damage = { itype::damage_scale, itype::damage_scale };
    item damageable( itype_id( "glock_19" ) );
    item undamageable( itype_id( "rock" ) );
    rng_set_engine_seed( 1 );
    damaged.modify( damageable, "Rust item-group damageable modifier oracle" );
    rng_set_engine_seed( 1 );
    damaged.modify( undamageable, "Rust item-group undamageable modifier oracle" );

    Item_modifier variant_modifier;
    variant_modifier.variant = "flag_shirt";
    item variant_item( itype_id( "tshirt" ) );
    rng_set_engine_seed( 1 );
    variant_modifier.modify( variant_item, "Rust item-group variant modifier oracle" );
    REQUIRE( variant_item.has_itype_variant() );

    Item_modifier dressed;
    dressed.with_ammo = 100;
    dressed.with_magazine = 100;
    item dressed_gun( itype_id( "glock_19" ) );
    rng_set_engine_seed( 1 );
    dressed.modify( dressed_gun, "Rust item-group detachable dressing oracle" );
    const item *dressed_magazine = dressed_gun.magazine_current();
    item dressed_integral_tool( itype_id( "matches" ) );
    rng_set_engine_seed( 1 );
    dressed.modify( dressed_integral_tool, "Rust item-group integral dressing oracle" );

    const container_observation discarded = observe_container_group(
                "discard", item_group_id( "test_truncating_to_container" ) );
    const container_observation spilled = observe_container_group(
                "spill", item_group_id( "test_spilling_from_container" ) );
    const corpse_observation corpses = observe_everyday_corpses();

    std::set<std::string> event_types;
    std::vector<std::pair<int, std::string>> event_distribution_results;
    {
        // 2021-02-28 or 2021-03-01 in supported host timezones, both outside
        // all pinned holiday windows. Priming the function's cache is necessary because
        // item-group event checks call get_holiday_from_time() with no argument.
        scoped_holiday_override nonholiday( 1614574800L );
        scoped_option_override event_spawns( "EVENT_SPAWNS", "items" );
        REQUIRE( nonholiday.value() == holiday::none );
        REQUIRE( get_holiday_from_time() == holiday::none );
        const item_group::ItemList event_items = item_group::items_from(
                    item_group_id( "test_event_item_spawn" ) );
        for( const item &event_item : event_items ) {
            event_types.insert( event_item.typeId().str() );
        }
        for( const int ticket : { 1, 3, 4, 5 } ) {
            const unsigned int seed = seed_for_first_draw( 1, 5, ticket );
            const std::vector<std::string> trace = event_distribution_trace( seed );
            REQUIRE( trace.size() <= 1 );
            event_distribution_results.emplace_back(
                ticket, trace.empty() ? "none" : trace.front() );
        }
    }

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

        json.member( "modifier_rng_phase" );
        json.start_object();
        json.member( "case_id", "direct_fixed_count" );
        json.member( "rolls_consumed", 4 );
        json.member( "expected_downstream", modifier_rng_expected_downstream );
        json.member( "actual_downstream", modifier_rng_actual_downstream );
        json.member( "downstream_draw_matches",
                     modifier_rng_expected_downstream == modifier_rng_actual_downstream );
        json.end_object();

        json.member( "constructor_variants" );
        json.start_array();
        for( const constructor_variant_trace &trace : constructor_variants ) {
            json.start_object();
            json.member( "seed", trace.seed );
            json.member( "selected", trace.selected );
            json.member( "name", trace.name );
            json.member( "description", trace.description );
            json.member( "downstream_draw", trace.downstream_draw );
            json.end_object();
        }
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

        json.member( "modifiers" );
        json.start_object();
        json.member( "damageable_raw_damage", damageable.damage() );
        json.member( "damageable_damage_level", damageable.damage_level() );
        json.member( "undamageable_raw_damage", undamageable.damage() );
        json.member( "explicit_variant", variant_item.itype_variant().id );
        json.member( "detachable_magazine_present", dressed_magazine != nullptr );
        json.member( "detachable_magazine_type",
                     dressed_magazine == nullptr ? "" : dressed_magazine->typeId().str() );
        json.member( "detachable_ammunition_type", dressed_gun.ammo_current().str() );
        json.member( "detachable_ammo_remaining", dressed_gun.ammo_remaining() );
        json.member( "detachable_remaining_capacity", dressed_gun.remaining_ammo_capacity() );
        json.member( "integral_ammo_remaining", dressed_integral_tool.ammo_remaining() );
        json.member( "integral_ammunition_type", dressed_integral_tool.ammo_current().str() );
        json.member( "integral_remaining_capacity", dressed_integral_tool.remaining_ammo_capacity() );
        json.end_object();

        json.member( "containers" );
        json.start_array();
        for( const container_observation *observation : { &discarded, &spilled } ) {
            json.start_object();
            json.member( "case_id", observation->case_id );
            json.member( "seed_search_limit", observation->seed_search_limit );
            json.member( "valid_shapes", observation->valid_shapes );
            json.member( "minimum_top_level", observation->minimum_top_level );
            json.member( "maximum_top_level", observation->maximum_top_level );
            json.member( "minimum_contents", observation->minimum_contents );
            json.member( "maximum_contents", observation->maximum_contents );
            json.member( "content_orders" );
            write_strings( json, observation->content_orders );
            json.member( "outside_types" );
            write_strings( json, observation->outside_types );
            json.member( "exact_traces" );
            json.start_array();
            for( const container_observation::exact_trace &trace : observation->exact_traces ) {
                json.start_object();
                json.member( "witness", trace.witness );
                json.member( "seed", trace.seed );
                json.member( "top_level_types" );
                write_trace( json, trace.top_level_types );
                json.member( "content_types" );
                write_trace( json, trace.content_types );
                json.end_object();
            }
            json.end_array();
            json.end_object();
        }
        json.end_array();

        json.member( "everyday_corpse" );
        json.start_object();
        json.member( "seed_search_limit", corpses.seed_search_limit );
        json.member( "valid_shapes", corpses.valid_shapes );
        json.member( "wrapper_types" );
        write_strings( json, corpses.wrapper_types );
        json.member( "wrapper_raw_damage" );
        json.start_array();
        for( const int damage : corpses.wrapper_raw_damage ) {
            json.write( damage );
        }
        json.end_array();
        json.member( "wrapper_damage_levels" );
        json.start_array();
        for( const int level : corpses.wrapper_damage_levels ) {
            json.write( level );
        }
        json.end_array();
        json.member( "multiple_content_counts", corpses.content_counts.size() > 1 );
        json.member( "observed_pristine_content", corpses.observed_pristine_content );
        json.member( "observed_damage_four_content", corpses.observed_damage_four_content );
        json.member( "exact_traces" );
        json.start_array();
        for( const corpse_observation::exact_trace &trace : corpses.exact_traces ) {
            json.start_object();
            json.member( "witness", trace.witness );
            json.member( "seed", trace.seed );
            json.member( "wrapper_type", trace.wrapper_type );
            json.member( "wrapper_raw_damage", trace.wrapper_raw_damage );
            json.member( "wrapper_damage_level", trace.wrapper_damage_level );
            json.member( "content_types" );
            write_trace( json, trace.content_types );
            json.member( "content_raw_damage" );
            json.start_array();
            for( const int damage : trace.content_raw_damage ) {
                json.write( damage );
            }
            json.end_array();
            json.member( "content_damage_levels" );
            json.start_array();
            for( const int level : trace.content_damage_levels ) {
                json.write( level );
            }
            json.end_array();
            json.end_object();
        }
        json.end_array();
        json.end_object();

        json.member( "nonholiday_event_types" );
        write_strings( json, event_types );

        json.member( "event_distribution" );
        json.start_array();
        for( const auto &[ticket, selected] : event_distribution_results ) {
            json.start_object();
            json.member( "ticket", ticket );
            json.member( "selected", selected );
            json.end_object();
        }
        json.end_array();
        json.end_object();
    }
    output.close();
    REQUIRE( output.good() );
}
