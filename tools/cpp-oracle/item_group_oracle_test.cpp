#include <algorithm>
#include <cmath>
#include <cstdint>
#include <ctime>
#include <cstdlib>
#include <fstream>
#include <limits>
#include <list>
#include <map>
#include <memory>
#include <set>
#include <sstream>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

#include "calendar.h"
#include "cata_catch.h"
#include "flag.h"
#include "item.h"
#include "item_group.h"
#include "item_pocket.h"
#include "itype.h"
#include "json.h"
#include "json_loader.h"
#include "options.h"
#include "pocket_type.h"
#include "rng.h"
#include "text_snippets.h"
#include "type_id.h"
#include "units.h"

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

struct tool_charge_trace {
    int requested_charges = 0;
    std::string tool_type;
    bool magazine_present = false;
    std::string magazine_type;
    std::string ammunition_type;
    int ammunition_remaining = 0;
    int remaining_capacity = 0;
};

struct magazine_charge_trace {
    std::string case_id;
    unsigned int seed = 0;
    int requested_charges = 0;
    std::string item_type;
    std::string ammunition_type;
    int ammunition_remaining = 0;
    int remaining_capacity = 0;
    int downstream_draw = 0;
};

struct magazine_charge_observation {
    std::string production_group;
    std::vector<magazine_charge_trace> direct;
    std::vector<magazine_charge_trace> production;
};

magazine_charge_trace observe_direct_magazine_charges( const std::string &case_id,
        const itype_id &item_type, int requested_charges )
{
    constexpr unsigned int seed = 8675309;
    Single_item_creator creator( item_type.str(), Single_item_creator::S_ITEM, 100,
                                 "Rust item-group integral magazine charge oracle" );
    creator.modifier.emplace();
    creator.modifier->charges = { requested_charges, requested_charges };
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( seed );
    creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    REQUIRE( items.size() == 1 );
    const item &magazine = items.front();
    return {
        case_id,
        seed,
        requested_charges,
        magazine.typeId().str(),
        magazine.ammo_current().str(),
        magazine.ammo_remaining(),
        magazine.remaining_ammo_capacity(),
        rng( 0, 9999 )
    };
}

magazine_charge_trace observe_production_magazine_charges( const std::string &case_id,
        const itype_id &item_type, int minimum_remaining, int maximum_remaining )
{
    const item_group_id group_id( "ammo_light_batteries" );
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        rng_set_engine_seed( seed );
        const item_group::ItemList items = item_group::items_from( group_id );
        if( items.size() != 1 || items.front().typeId() != item_type ) {
            continue;
        }
        const item &magazine = items.front();
        const int remaining = magazine.ammo_remaining();
        if( remaining < minimum_remaining || remaining > maximum_remaining ) {
            continue;
        }
        return {
            case_id,
            seed,
            -1,
            magazine.typeId().str(),
            magazine.ammo_current().str(),
            remaining,
            magazine.remaining_ammo_capacity(),
            rng( 0, 9999 )
        };
    }
    FAIL( "could not find bounded production integral-magazine charge witness" );
    return {};
}

magazine_charge_observation observe_magazine_charges()
{
    magazine_charge_observation observation;
    observation.production_group = "ammo_light_batteries";
    for( const int requested : { 0, 1, 16, 100 } ) {
        observation.direct.push_back( observe_direct_magazine_charges(
                                          string_format( "light_%d", requested ),
                                          itype_id( "light_battery_cell" ), requested ) );
    }
    observation.direct.push_back( observe_direct_magazine_charges(
                                      "ultralight_overflow", itype_id( "light_minus_battery_cell" ), 100 ) );
    observation.production.push_back( observe_production_magazine_charges(
                                          "production_empty_light", itype_id( "light_battery_cell" ), 0, 0 ) );
    observation.production.push_back( observe_production_magazine_charges(
                                          "production_partial_light", itype_id( "light_battery_cell" ), 1, 15 ) );
    observation.production.push_back( observe_production_magazine_charges(
                                          "production_full_light", itype_id( "light_battery_cell" ), 16, 16 ) );
    observation.production.push_back( observe_production_magazine_charges(
                                          "production_full_ultralight", itype_id( "light_minus_battery_cell" ), 2, 2 ) );
    return observation;
}

tool_charge_trace observe_tool_charges( int requested_charges )
{
    Single_item_creator creator( "wearable_light", Single_item_creator::S_ITEM, 100,
                                 "Rust item-group detachable tool charge oracle" );
    creator.modifier.emplace();
    creator.modifier->charges = { requested_charges, requested_charges };
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( 8675309 );
    creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    REQUIRE( items.size() == 1 );
    const item &tool = items.front();
    const item *magazine = tool.magazine_current();
    return {
        requested_charges,
        tool.typeId().str(),
        magazine != nullptr,
        magazine == nullptr ? "" : magazine->typeId().str(),
        tool.ammo_current().str(),
        tool.ammo_remaining(),
        tool.remaining_ammo_capacity()
    };
}

struct dressing_trace {
    std::string case_id;
    std::string source_group;
    unsigned int seed = 0;
    int ammunition_chance = 0;
    int magazine_chance = 0;
    int charges_minimum = -1;
    int charges_maximum = -1;
    std::string item_type;
    bool magazine_present = false;
    std::string magazine_type;
    std::string ammunition_type;
    int ammunition_remaining = 0;
    int remaining_capacity = 0;
    int downstream_draw = 0;
};

dressing_trace dressing_result( const std::string &case_id, const std::string &source_group,
                                unsigned int seed, int ammunition_chance,
                                int magazine_chance, int charges_minimum,
                                int charges_maximum, const item &observed )
{
    const item *magazine = observed.magazine_current();
    return {
        case_id,
        source_group,
        seed,
        ammunition_chance,
        magazine_chance,
        charges_minimum,
        charges_maximum,
        observed.typeId().str(),
        magazine != nullptr,
        magazine == nullptr ? "" : magazine->typeId().str(),
        observed.ammo_current().str(),
        observed.ammo_remaining(),
        observed.remaining_ammo_capacity(),
        rng( 0, 9999 )
    };
}

dressing_trace observe_direct_dressing( const std::string &case_id,
        const std::string &item_type, unsigned int seed, int ammunition_chance,
        int magazine_chance, int charges_minimum = -1, int charges_maximum = -1 )
{
    Single_item_creator creator( item_type, Single_item_creator::S_ITEM, 100,
                                 "Rust item-group dressing oracle" );
    creator.modifier.emplace();
    creator.modifier->with_ammo = ammunition_chance;
    creator.modifier->with_magazine = magazine_chance;
    creator.modifier->charges = { charges_minimum, charges_maximum };
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( seed );
    creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    REQUIRE( items.size() == 1 );
    return dressing_result( case_id, "", seed, ammunition_chance, magazine_chance,
                            charges_minimum, charges_maximum, items.front() );
}

dressing_trace find_direct_dressing( const std::string &case_id,
                                     const std::string &item_type,
                                     int ammunition_chance, int magazine_chance,
                                     int expected_ammunition_remaining )
{
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        const dressing_trace trace = observe_direct_dressing(
                                         case_id, item_type, seed,
                                         ammunition_chance, magazine_chance );
        if( trace.ammunition_remaining == expected_ammunition_remaining ) {
            return trace;
        }
    }
    FAIL( "could not find bounded direct dressing chance witness" );
    return {};
}

dressing_trace find_production_dressing( const std::string &case_id,
        const std::string &source_group, const std::string &item_type,
        int expected_ammunition_remaining )
{
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        rng_set_engine_seed( seed );
        const item_group::ItemList items = item_group::items_from( item_group_id( source_group ) );
        const auto found = std::find_if( items.begin(), items.end(), [&]( const item &candidate ) {
            return candidate.typeId() == itype_id( item_type ) &&
                   candidate.ammo_remaining() == expected_ammunition_remaining;
        } );
        if( found != items.end() ) {
            const bool everyday_gear = source_group == "everyday_gear";
            const int ammunition_chance = everyday_gear ? 75 : 0;
            const int magazine_chance = everyday_gear ? 100 : 0;
            const int charges_minimum = item_type == "inhaler" ? 10 :
                                        item_type == "teargas_sprayer" ? 1 : 0;
            const int charges_maximum = item_type == "matches" ? 20 :
                                        item_type == "ref_matches" ? 32 :
                                        item_type == "inhaler" ? 100 :
                                        item_type == "flashlight" ? 300 :
                                        item_type == "teargas_sprayer" ? 10 : -1;
            return dressing_result( case_id, source_group, seed, ammunition_chance,
                                    magazine_chance, charges_minimum, charges_maximum, *found );
        }
    }
    FAIL( "could not find bounded production dressing witness" );
    return {};
}

struct repeated_tool_charge_trace {
    std::string source_group;
    unsigned int seed = 0;
    int leaf_minimum = 0;
    int leaf_maximum = 0;
    int replacement_requested = 0;
    std::string tool_type;
    std::string magazine_type;
    std::string ammunition_type;
    int ammunition_remaining = 0;
    int downstream_draw = 0;
};

repeated_tool_charge_trace observe_repeated_tool_charges()
{
    constexpr const char *source_group = "accesories_personal_unisex_child";
    constexpr int replacement_requested = 1;
    unsigned int selected_seed = 0;
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        Single_item_creator selector( source_group, Single_item_creator::S_ITEM_GROUP, 100,
                                      "Rust repeated detachable tool charge seed search" );
        Item_spawn_data::ItemList items;
        Item_spawn_data::RecursionList recursion;
        rng_set_engine_seed( seed );
        selector.create( items, calendar::turn_zero, recursion, spawn_flags::none );
        if( items.size() == 1 && items.front().typeId() == itype_id( "wearable_light" ) ) {
            selected_seed = seed;
            break;
        }
    }
    if( selected_seed == 0 ) {
        FAIL( "could not select the repeated detachable tool-charge witness" );
        return {};
    }
    Single_item_creator creator( source_group, Single_item_creator::S_ITEM_GROUP, 100,
                                 "Rust repeated detachable tool charge oracle" );
    creator.modifier.emplace();
    creator.modifier->charges = { replacement_requested, replacement_requested };
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( selected_seed );
    creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    if( items.size() != 1 || items.front().typeId() != itype_id( "wearable_light" ) ||
        items.front().magazine_current() == nullptr ) {
        FAIL( "repeated detachable tool-charge witness changed after modifier application" );
        return {};
    }
    const item &tool = items.front();
    const item &magazine = *tool.magazine_current();
    return {
        source_group,
        selected_seed,
        0,
        100,
        replacement_requested,
        tool.typeId().str(),
        magazine.typeId().str(),
        tool.ammo_current().str(),
        tool.ammo_remaining(),
        rng( 0, 9999 )
    };
}

struct modifier_container_capacity_trace {
    unsigned int seed = 0;
    std::string container_type;
    std::string payload_type;
    int payload_charges = 0;
    int downstream_draw = 0;
};

struct charge_capacity_sentinel_trace {
    std::string case_id;
    unsigned int seed = 0;
    int minimum = 0;
    int maximum = 0;
    int effective_minimum = 0;
    int effective_maximum = 0;
    std::string item_type;
    int item_charges = 0;
    std::string ammunition_type;
    int ammunition_remaining = 0;
    int remaining_capacity = 0;
    bool magazine_present = false;
    std::string magazine_type;
    std::string wrapper_type;
    int downstream_draw = 0;
};

struct default_container_trace {
    std::string case_id;
    unsigned int seed = 0;
    std::string outer_type;
    std::vector<std::string> content_types;
    int payload_charges = -1;
    bool sealed = false;
    bool pocket_collapsed = false;
    int downstream_draw = 0;
};

struct flexible_wrapper_trace {
    std::string case_id;
    unsigned int seed = 0;
    std::string outer_type;
    std::string outer_variant;
    bool pocket_rigid = false;
    bool pocket_collapsed_by_default = false;
    bool pocket_collapsed = false;
    std::vector<std::string> content_types;
    std::vector<std::string> content_variants;
    std::vector<int> content_charges;
    std::int64_t outer_volume_ml = 0;
    std::int64_t outer_weight_g = 0;
    std::int64_t pocket_capacity_volume_ml = 0;
    std::int64_t pocket_remaining_volume_ml = 0;
    std::int64_t pocket_remaining_weight_g = 0;
    bool sealed = false;
    int downstream_draw = 0;
};

struct temperature_constructor_trace {
    std::string case_id;
    std::string item_type;
    int birth_turn;
    bool has_temperature = false;
    bool active = false;
    int processing_speed = 0;
    int temperature_millikelvin = 0;
    int specific_energy_millijoules_per_gram = 0;
    bool thermal_properties_present = false;
    std::int64_t specific_heat_liquid_microjoules_per_gram_kelvin = 0;
    std::int64_t specific_heat_solid_microjoules_per_gram_kelvin = 0;
    std::int64_t latent_heat_microjoules_per_gram = 0;
    int freezing_point_millikelvin = 0;
    int ambient_specific_energy_millijoules_per_gram = 0;
    bool serialized_last_temp_check_present = false;
    int serialized_last_temp_check = 0;
    bool solid = false;
    bool liquid = false;
    bool hot = false;
    bool cold = false;
    bool frozen = false;
};

temperature_constructor_trace observe_temperature_constructor( const std::string &case_id,
        const std::string &item_id, int birth_turn )
{
    const time_point birthday = calendar::turn_zero + time_duration::from_turns( birth_turn );
    const item observed( itype_id( item_id ), birthday );
    std::ostringstream serialized;
    {
        JsonOut json( serialized );
        observed.serialize( json );
    }
    JsonObject object = json_loader::from_string( serialized.str() ).get_object();
    const bool has_last_temp_check = object.has_int( "last_temp_check" );
    object.allow_omitted_members();
    const double specific_heat_liquid = observed.has_temperature() ?
                                        observed.get_specific_heat_liquid() : 0.0;
    const double specific_heat_solid = observed.has_temperature() ?
                                       observed.get_specific_heat_solid() : 0.0;
    const double latent_heat = observed.has_temperature() ? observed.get_latent_heat() : 0.0;
    const double freezing_point = observed.has_temperature() ?
                                  units::to_kelvin( observed.get_freeze_point() ) : 0.0;
    const bool thermal_properties_present = observed.has_temperature() &&
                                            std::isfinite( specific_heat_liquid ) &&
                                            std::isfinite( specific_heat_solid ) &&
                                            std::isfinite( latent_heat ) &&
                                            std::isfinite( freezing_point );
    int ambient_specific_energy = 0;
    if( thermal_properties_present ) {
        item at_ambient = observed;
        at_ambient.set_item_temperature( units::from_celsius( 20.0 ) );
        ambient_specific_energy = static_cast<int>( std::lround( units::to_joule_per_gram(
                                      at_ambient.specific_energy ) * 1000.0 ) );
    }
    return {
        case_id,
        item_id,
        birth_turn,
        observed.has_temperature(),
        observed.is_active(),
        observed.processing_speed(),
        static_cast<int>( std::lround( units::to_kelvin( observed.temperature ) * 1000.0 ) ),
        static_cast<int>( std::lround( units::to_joule_per_gram( observed.specific_energy ) *
                                      1000.0 ) ),
        thermal_properties_present,
        thermal_properties_present ? static_cast<std::int64_t>( std::llround(
                    specific_heat_liquid * 1000000.0 ) ) : 0,
        thermal_properties_present ? static_cast<std::int64_t>( std::llround(
                    specific_heat_solid * 1000000.0 ) ) : 0,
        thermal_properties_present ? static_cast<std::int64_t>( std::llround(
                    latent_heat * 1000000.0 ) ) : 0,
        thermal_properties_present ? static_cast<int>( std::lround( freezing_point * 1000.0 ) ) : 0,
        ambient_specific_energy,
        has_last_temp_check,
        has_last_temp_check ? object.get_int( "last_temp_check" ) : 0,
        observed.made_of( phase_id::SOLID ),
        observed.made_of( phase_id::LIQUID ),
        observed.has_own_flag( flag_HOT ),
        observed.has_own_flag( flag_COLD ),
        observed.has_own_flag( flag_FROZEN )
    };
}

enum class default_container_mode {
    unmodified,
    modifier_fallback,
    modifier_suppressed,
    explicit_container_default
};

default_container_trace observe_default_container( const std::string &case_id,
        const std::string &item_id, unsigned int seed, default_container_mode mode )
{
    Single_item_creator creator( item_id, Single_item_creator::S_ITEM, 100,
                                 "Rust default-container oracle" );
    if( mode != default_container_mode::unmodified ) {
        creator.modifier.emplace();
    }
    if( mode == default_container_mode::modifier_suppressed ) {
        creator.modifier->container = std::make_unique<Single_item_creator>(
                                          "null", Single_item_creator::S_ITEM, 100,
                                          "Rust default-container null suppression oracle" );
    } else if( mode == default_container_mode::explicit_container_default ) {
        creator.modifier->container = std::make_unique<Single_item_creator>(
                                          "aspirin", Single_item_creator::S_ITEM, 100,
                                          "Rust nested explicit default-container oracle" );
    }
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( seed );
    creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    REQUIRE( items.size() == 1 );
    const item &outer = items.front();
    const std::list<const item *> contents = outer.all_items_top();
    const std::vector<const item_pocket *> pockets = outer.get_pockets(
                []( const item_pocket & pocket ) {
        return pocket.is_type( pocket_type::CONTAINER );
    } );
    std::vector<std::string> content_types;
    int payload_charges = -1;
    for( const item *content : contents ) {
        content_types.push_back( content->typeId().str() );
        if( payload_charges == -1 ) {
            payload_charges = content->charges;
        }
    }
    return {
        case_id,
        seed,
        outer.typeId().str(),
        std::move( content_types ),
        payload_charges,
        outer.any_pockets_sealed(),
        pockets.size() == 1 && pockets.front()->settings.is_collapsed(),
        rng( 0, 9999 )
    };
}

default_container_trace observe_painkiller_group_boundary( const std::string &case_id,
        std::size_t expected_count )
{
    constexpr const char *group_id = "bottle_otc_painkiller_1_20";
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        Single_item_creator creator( group_id, Single_item_creator::S_ITEM_GROUP, 100,
                                     "Rust production default-container oracle" );
        Item_spawn_data::ItemList items;
        Item_spawn_data::RecursionList recursion;
        rng_set_engine_seed( seed );
        creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
        if( items.size() != 1 || items.front().typeId() !=
            itype_id( "bottle_plastic_pill_painkiller" ) ) {
            continue;
        }
        const item &outer = items.front();
        const std::list<const item *> contents = outer.all_items_top();
        const std::vector<const item_pocket *> pockets = outer.get_pockets(
                    []( const item_pocket & pocket ) {
            return pocket.is_type( pocket_type::CONTAINER );
        } );
        if( contents.size() != expected_count ||
            std::any_of( contents.begin(), contents.end(), []( const item * content ) {
            return content->typeId() != itype_id( "aspirin" );
        } ) ) {
            continue;
        }
        std::vector<std::string> content_types( expected_count, "aspirin" );
        return {
            case_id,
            seed,
            outer.typeId().str(),
            std::move( content_types ),
            contents.empty() ? -1 : ( *contents.begin() )->charges,
            outer.any_pockets_sealed(),
            pockets.size() == 1 && pockets.front()->settings.is_collapsed(),
            rng( 0, 9999 )
        };
    }
    FAIL( "could not find production painkiller default-container boundary" );
    return {};
}

flexible_wrapper_trace observe_chaw_wrapper_boundary( const std::string &case_id,
        std::size_t expected_count )
{
    constexpr const char *group_id = "chaw_wrapper_1_20";
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        Single_item_creator creator( group_id, Single_item_creator::S_ITEM_GROUP, 100,
                                     "Rust production flexible-wrapper oracle" );
        Item_spawn_data::ItemList items;
        Item_spawn_data::RecursionList recursion;
        rng_set_engine_seed( seed );
        creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
        if( items.size() != 1 || items.front().typeId() != itype_id( "wrapper" ) ) {
            continue;
        }
        const item &outer = items.front();
        const std::list<const item *> contents = outer.all_items_top( pocket_type::CONTAINER );
        const std::vector<const item_pocket *> pockets = outer.get_pockets(
                    []( const item_pocket & pocket ) {
            return pocket.is_type( pocket_type::CONTAINER );
        } );
        if( contents.size() != expected_count || pockets.size() != 1 ||
            std::any_of( contents.begin(), contents.end(), []( const item * content ) {
            return content->typeId() != itype_id( "chaw" );
        } ) ) {
            continue;
        }
        std::vector<std::string> content_types;
        std::vector<std::string> content_variants;
        std::vector<int> content_charges;
        content_types.reserve( contents.size() );
        content_variants.reserve( contents.size() );
        content_charges.reserve( contents.size() );
        for( const item *content : contents ) {
            content_types.push_back( content->typeId().str() );
            content_variants.push_back( content->has_itype_variant( false ) ?
                                        content->itype_variant().id : "" );
            content_charges.push_back( content->charges );
        }
        const item_pocket &pocket = *pockets.front();
        return {
            case_id,
            seed,
            outer.typeId().str(),
            outer.has_itype_variant( false ) ? outer.itype_variant().id : "",
            pocket.rigid(),
            outer.has_flag( flag_COLLAPSE_CONTENTS ),
            pocket.settings.is_collapsed(),
            std::move( content_types ),
            std::move( content_variants ),
            std::move( content_charges ),
            units::to_milliliter( outer.volume() ),
            units::to_gram( outer.weight() ),
            units::to_milliliter( pocket.volume_capacity() ),
            units::to_milliliter( pocket.remaining_volume() ),
            units::to_gram( pocket.remaining_weight() ),
            outer.any_pockets_sealed(),
            rng( 0, 9999 )
        };
    }
    FAIL( "could not find production flexible-wrapper boundary" );
    return {};
}

flexible_wrapper_trace observe_chewing_gum_wrapper()
{
    constexpr const char *group_id = "chewing_gum_full";
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        Single_item_creator creator( group_id, Single_item_creator::S_ITEM_GROUP, 100,
                                     "Rust production collapsed-wrapper oracle" );
        Item_spawn_data::ItemList items;
        Item_spawn_data::RecursionList recursion;
        rng_set_engine_seed( seed );
        creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
        if( items.size() != 1 || items.front().typeId() != itype_id( "blister_pack_small" ) ) {
            continue;
        }
        const item &outer = items.front();
        const std::list<const item *> contents = outer.all_items_top( pocket_type::CONTAINER );
        const std::vector<const item_pocket *> pockets = outer.get_pockets(
                    []( const item_pocket & pocket ) {
            return pocket.is_type( pocket_type::CONTAINER );
        } );
        if( contents.size() != 12 || pockets.size() != 1 ||
            std::any_of( contents.begin(), contents.end(), []( const item * content ) {
            return content->typeId() != itype_id( "gum" );
        } ) ) {
            continue;
        }
        std::vector<std::string> content_types;
        std::vector<std::string> content_variants;
        std::vector<int> content_charges;
        for( const item *content : contents ) {
            content_types.push_back( content->typeId().str() );
            content_variants.push_back( content->has_itype_variant( false ) ?
                                        content->itype_variant().id : "" );
            content_charges.push_back( content->charges );
        }
        const item_pocket &pocket = *pockets.front();
        return {
            "production_chewing_gum",
            seed,
            outer.typeId().str(),
            outer.has_itype_variant( false ) ? outer.itype_variant().id : "",
            pocket.rigid(),
            outer.has_flag( flag_COLLAPSE_CONTENTS ),
            pocket.settings.is_collapsed(),
            std::move( content_types ),
            std::move( content_variants ),
            std::move( content_charges ),
            units::to_milliliter( outer.volume() ),
            units::to_gram( outer.weight() ),
            units::to_milliliter( pocket.volume_capacity() ),
            units::to_milliliter( pocket.remaining_volume() ),
            units::to_gram( pocket.remaining_weight() ),
            outer.any_pockets_sealed(),
            rng( 0, 9999 )
        };
    }
    FAIL( "could not find production collapsed chewing-gum wrapper" );
    return {};
}

modifier_container_capacity_trace observe_modifier_container_capacity( unsigned int seed,
        int minimum, int maximum )
{
    Single_item_creator creator( "water_clean", Single_item_creator::S_ITEM, 100,
                                 "Rust modifier-container capacity oracle" );
    creator.modifier.emplace();
    creator.modifier->container = std::make_unique<Single_item_creator>(
                                      "bottle_plastic", Single_item_creator::S_ITEM, 100,
                                      "Rust modifier-container capacity oracle wrapper" );
    creator.modifier->sealed = false;
    if( minimum >= 0 || maximum >= 0 ) {
        creator.modifier->charges = { minimum, maximum };
    }
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( seed );
    creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    REQUIRE( items.size() == 1 );
    const item &container = items.front();
    const std::list<const item *> contents = container.all_items_top();
    REQUIRE( contents.size() == 1 );
    const item &payload = **contents.begin();
    return {
        seed,
        container.typeId().str(),
        payload.typeId().str(),
        payload.charges,
        rng( 0, 9999 )
    };
}

std::pair<int, int> effective_charge_capacity_bounds( const std::string &item_id,
        int minimum, int maximum, const std::string &container_id )
{
    item observed( itype_id( item_id ), calendar::turn_zero );
    std::optional<item> container;
    if( !container_id.empty() ) {
        container.emplace( itype_id( container_id ), calendar::turn_zero );
    }
    int max_capacity = -1;
    if( minimum != -1 && maximum == -1 && ( observed.is_magazine() ||
            observed.uses_magazine() ) ) {
        int max_ammo = 0;
        if( observed.is_magazine() ) {
            if( const std::optional<ammotype> at = item::ammotype_of( observed.ammo_default() ) ) {
                max_ammo = observed.ammo_capacity( *at );
            }
        } else if( !observed.magazine_default().is_null() ) {
            const itype *magazine = item::find_type( observed.magazine_default() );
            if( magazine != nullptr && magazine->magazine ) {
                max_ammo = magazine->magazine->capacity;
            }
        }
        if( max_ammo > 0 ) {
            max_capacity = max_ammo;
        }
    }
    if( max_capacity == -1 && container.has_value() &&
        ( observed.made_of( phase_id::LIQUID ) ||
          ( !observed.is_tool() && !observed.is_gun() && !observed.is_magazine() ) ) ) {
        if( observed.type->weight == 0_gram ) {
            max_capacity = observed.charges_per_volume( container->get_volume_capacity() );
        } else {
            max_capacity = std::min(
                               observed.charges_per_volume( container->get_volume_capacity() ),
                               observed.charges_per_weight( container->get_total_weight_capacity() ) );
        }
    }
    if( minimum == -1 && maximum == -1 ) {
        return { -1, -1 };
    }
    int effective_minimum = minimum == -1 ? 0 : minimum;
    int effective_maximum = maximum == -1 ? max_capacity : maximum;
    if( effective_minimum == -1 && effective_maximum != -1 ) {
        effective_minimum = 0;
    }
    if( max_capacity != -1 && ( effective_maximum > max_capacity ||
                                ( effective_minimum != 1 && effective_maximum == -1 ) ) ) {
        effective_maximum = max_capacity;
    }
    if( effective_minimum > effective_maximum ) {
        effective_minimum = effective_maximum;
    }
    return { effective_minimum, effective_maximum };
}

charge_capacity_sentinel_trace observe_charge_capacity_sentinel(
    const std::string &case_id, const std::string &item_id, unsigned int seed,
    int minimum, int maximum, const std::string &container_id = "" )
{
    const auto [effective_minimum, effective_maximum] =
        effective_charge_capacity_bounds( item_id, minimum, maximum, container_id );
    Single_item_creator creator( item_id, Single_item_creator::S_ITEM, 100,
                                 "Rust charge-capacity sentinel oracle" );
    creator.modifier.emplace();
    creator.modifier->charges = { minimum, maximum };
    if( !container_id.empty() ) {
        creator.modifier->container = std::make_unique<Single_item_creator>(
                                          container_id, Single_item_creator::S_ITEM, 100,
                                          "Rust charge-capacity sentinel wrapper oracle" );
        creator.modifier->sealed = false;
    }
    Item_spawn_data::ItemList items;
    Item_spawn_data::RecursionList recursion;
    rng_set_engine_seed( seed );
    creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
    REQUIRE( items.size() == 1 );
    const item &outer = items.front();
    const item *observed = &outer;
    if( !container_id.empty() ) {
        const std::list<const item *> contents = outer.all_items_top();
        REQUIRE( contents.size() == 1 );
        observed = *contents.begin();
    }
    const item *magazine = observed->magazine_current();
    return {
        case_id,
        seed,
        minimum,
        maximum,
        effective_minimum,
        effective_maximum,
        observed->typeId().str(),
        observed->charges,
        observed->ammo_current().str(),
        observed->ammo_remaining(),
        observed->remaining_ammo_capacity(),
        magazine != nullptr,
        magazine == nullptr ? "" : magazine->typeId().str(),
        container_id.empty() ? "" : outer.typeId().str(),
        rng( 0, 9999 )
    };
}

charge_capacity_sentinel_trace find_charge_capacity_sentinel(
    const std::string &case_id, const std::string &item_id,
    int minimum, int maximum, int target_ammunition_remaining,
    const std::string &container_id = "",
    int target_item_charges = std::numeric_limits<int>::min() )
{
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        charge_capacity_sentinel_trace trace = observe_charge_capacity_sentinel(
                case_id, item_id, seed, minimum, maximum, container_id );
        if( trace.ammunition_remaining == target_ammunition_remaining &&
            ( target_item_charges == std::numeric_limits<int>::min() ||
              trace.item_charges == target_item_charges ) ) {
            return trace;
        }
    }
    FAIL( "could not find bounded charge-capacity sentinel witness for " + case_id );
    return {};
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

struct description_expansion_trace {
    std::string direct_input;
    std::string direct_output;
    int direct_downstream_draw = 0;
    std::string source_group;
    unsigned int seed = 0;
    std::string item_type;
    std::string variant_id;
    std::string expanded_description;
    int downstream_draw = 0;
};

struct variable_size_fit_trace {
    std::string case_id;
    unsigned int seed = 0;
    std::string item_type;
    bool variable_size = false;
    bool fitted = false;
    std::string name;
    int downstream_draw = 0;
};

struct variable_size_fit_observation {
    std::string production_group;
    std::vector<variable_size_fit_trace> direct;
    std::vector<variable_size_fit_trace> production;
};

variable_size_fit_trace observe_direct_fit( const std::string &case_id,
        const itype_id &item_type, bool target_fitted, unsigned int first_seed = 1 )
{
    for( unsigned int seed = first_seed; seed <= maximum_seed_search; ++seed ) {
        Single_item_creator creator( item_type.str(), Single_item_creator::S_ITEM, 100,
                                     "Rust item-group variable-size FIT oracle" );
        Item_spawn_data::ItemList items;
        Item_spawn_data::RecursionList recursion;
        rng_set_engine_seed( seed );
        creator.create( items, calendar::turn_zero, recursion, spawn_flags::none );
        REQUIRE( items.size() == 1 );
        const item &generated = items.front();
        if( generated.has_flag( flag_FIT ) != target_fitted ) {
            continue;
        }
        return {
            case_id,
            seed,
            generated.typeId().str(),
            generated.has_flag( flag_VARSIZE ),
            generated.has_flag( flag_FIT ),
            generated.tname( 1, false ),
            rng( 0, 9999 )
        };
    }
    FAIL( "could not find a bounded direct variable-size FIT witness" );
    return {};
}

variable_size_fit_trace observe_production_fit( const std::string &case_id,
        const item_group_id &group_id, const itype_id &item_type, bool target_fitted )
{
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        rng_set_engine_seed( seed );
        const item_group::ItemList items = item_group::items_from( group_id );
        if( items.size() != 1 || items.front().typeId() != item_type ||
            items.front().has_flag( flag_FIT ) != target_fitted ) {
            continue;
        }
        const item &generated = items.front();
        return {
            case_id,
            seed,
            generated.typeId().str(),
            generated.has_flag( flag_VARSIZE ),
            generated.has_flag( flag_FIT ),
            generated.tname( 1, false ),
            rng( 0, 9999 )
        };
    }
    FAIL( "could not find a bounded production variable-size FIT witness" );
    return {};
}

variable_size_fit_observation observe_variable_size_fit()
{
    variable_size_fit_observation observation;
    observation.production_group = "accessory_weaponcarry";
    observation.direct.push_back( observe_direct_fit(
                                      "non_variable_control", itype_id( "test_pipe" ), false, 2 ) );
    observation.direct.push_back( observe_direct_fit(
                                      "variable_unfitted", itype_id( "leg_sheath6" ), false ) );
    observation.direct.push_back( observe_direct_fit(
                                      "variable_fitted", itype_id( "leg_sheath6" ), true ) );
    observation.production.push_back( observe_production_fit(
                                          "production_unfitted", item_group_id( observation.production_group ),
                                          itype_id( "leg_sheath6" ), false ) );
    observation.production.push_back( observe_production_fit(
                                          "production_fitted", item_group_id( observation.production_group ),
                                          itype_id( "leg_sheath6" ), true ) );
    return observation;
}

description_expansion_trace observe_description_expansion()
{
    description_expansion_trace trace;
    trace.direct_input = "Foo <lt>lt<gt> <unknown>";
    rng_set_engine_seed( 113 );
    trace.direct_output = SNIPPET.expand( trace.direct_input );
    trace.direct_downstream_draw = rng( 0, 9999 );
    REQUIRE( trace.direct_output == "Foo <lt> <unknown>" );

    trace.source_group = "accessory_necklace";
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        rng_set_engine_seed( seed );
        const item_group::ItemList items = item_group::items_from(
                    item_group_id( trace.source_group ) );
        if( items.size() != 1 ) {
            continue;
        }
        const item &generated = items.front();
        if( generated.typeId() != itype_id( "holy_symbol" ) ||
            !generated.has_itype_variant() ||
            generated.itype_variant().id != "saint_necklace" ) {
            continue;
        }
        trace.seed = seed;
        trace.item_type = generated.typeId().str();
        trace.variant_id = generated.itype_variant().id;
        trace.expanded_description = generated.get_var( "description" );
        trace.downstream_draw = rng( 0, 9999 );
        break;
    }
    REQUIRE( trace.seed > 0 );
    REQUIRE( trace.item_type == "holy_symbol" );
    REQUIRE( trace.variant_id == "saint_necklace" );
    REQUIRE( trace.expanded_description.find( "A necklace made of a fine gold chain" ) == 0 );
    REQUIRE( trace.expanded_description.find( "<catholic_saints>" ) == std::string::npos );
    return trace;
}

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

struct multi_pocket_trace {
    std::string case_id;
    unsigned int seed = 0;
    std::string wrapper_type;
    std::string payload_type;
    std::vector<std::vector<std::string>> pocket_contents;
    int downstream_draw = 0;
};

struct named_snippet_selection_trace {
    unsigned int seed = 0;
    std::string snippet_id;
    std::string text;
    int downstream_draw = 0;
};

struct named_snippet_category_trace {
    std::string case_id;
    std::string item_type;
    std::string category;
    std::vector<std::string> choice_ids;
    std::string first_text;
    std::string last_text;
    named_snippet_selection_trace first_selection;
    named_snippet_selection_trace last_selection;
};

std::string serialized_snippet_id( const item &observed )
{
    std::ostringstream serialized;
    {
        JsonOut json( serialized );
        observed.serialize( json );
    }
    JsonObject object = json_loader::from_string( serialized.str() ).get_object();
    object.allow_omitted_members();
    return object.has_string( "snip_id" ) ? object.get_string( "snip_id" ) : "";
}

named_snippet_category_trace observe_named_snippet_category( const std::string &case_id,
        const itype_id &item_type, const std::string &category )
{
    const std::vector<std::pair<snippet_id, std::string>> choices =
        SNIPPET.get_snippets_by_category( category );
    if( choices.empty() ) {
        FAIL( "production named snippet category has no identified choices" );
        return {};
    }
    const auto observe_selection = [&]( const snippet_id & target ) {
        for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
            rng_set_engine_seed( seed );
            const item observed( item_type, calendar::turn_zero );
            const std::string selected = serialized_snippet_id( observed );
            if( selected == target.str() ) {
                return named_snippet_selection_trace {
                    seed,
                    selected,
                    SNIPPET.get_snippet_ref_by_id( target ).translated(),
                    rng( 0, 9999 )
                };
            }
        }
        FAIL( "could not find exact production named-snippet boundary witness" );
        return named_snippet_selection_trace {};
    };
    std::vector<std::string> choice_ids;
    choice_ids.reserve( choices.size() );
    for( const auto &[id, text] : choices ) {
        static_cast<void>( text );
        choice_ids.push_back( id.str() );
    }
    return {
        case_id,
        item_type.str(),
        category,
        std::move( choice_ids ),
        choices.front().second,
        choices.back().second,
        observe_selection( choices.front().first ),
        observe_selection( choices.back().first )
    };
}

multi_pocket_trace observe_multi_pocket_wrapper( const std::string &case_id,
        const item_group_id &group_id, const itype_id &wrapper_type,
        const itype_id &payload_type, std::size_t target_contents )
{
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        rng_set_engine_seed( seed );
        const item_group::ItemList items = item_group::items_from( group_id );
        if( items.size() != 1 || items.front().typeId() != wrapper_type ) {
            continue;
        }
        std::vector<std::vector<std::string>> pocket_contents;
        std::size_t content_count = 0;
        bool valid_payloads = true;
        for( const item_pocket *pocket : items.front().get_contents().get_container_pockets() ) {
            std::vector<std::string> contents;
            for( const item *content : pocket->all_items_top() ) {
                contents.push_back( content->typeId().str() );
                valid_payloads = valid_payloads && content->typeId() == payload_type;
                ++content_count;
            }
            pocket_contents.push_back( std::move( contents ) );
        }
        if( valid_payloads && content_count == target_contents ) {
            return {
                case_id,
                seed,
                wrapper_type.str(),
                payload_type.str(),
                std::move( pocket_contents ),
                rng( 0, 9999 )
            };
        }
    }
    FAIL( "could not find exact production multi-pocket wrapper witness" );
    return {};
}

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

struct phone_case_observation {
    int seed_search_limit = 0;
    bool valid_shapes = true;
    std::set<std::string> phone_types;
    bool observed_empty_efiles = false;
    bool observed_many_efiles = false;
    struct exact_trace {
        std::string witness;
        unsigned int seed = 0;
        std::string wrapper_type;
        std::string wrapper_variant;
        bool wrapper_any_pocket_sealed = false;
        std::int64_t wrapper_remaining_volume_ml = 0;
        std::int64_t wrapper_remaining_weight_g = 0;
        std::string phone_type;
        int phone_charges = 0;
        int phone_ammo_remaining = 0;
        std::string phone_ammunition_type;
        int phone_raw_damage = 0;
        std::vector<std::string> efile_types;
        std::vector<int> efile_raw_damage;
        int downstream_draw = 0;
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

phone_case_observation observe_civilian_phone_cases()
{
    phone_case_observation observation;
    observation.seed_search_limit = maximum_seed_search;
    std::set<std::string> retained_witnesses;
    for( unsigned int seed = 1; seed <= maximum_seed_search; ++seed ) {
        rng_set_engine_seed( seed );
        const item_group::ItemList items = item_group::items_from(
                    item_group_id( "civilian_phones_case" ) );
        if( items.size() != 1 || items.front().typeId() != itype_id( "waterproof_smart_phone_case" ) ) {
            observation.valid_shapes = false;
            break;
        }
        const item &wrapper = items.front();
        const std::list<const item *> wrapper_contents = wrapper.all_items_top(
                    pocket_type::CONTAINER );
        const std::vector<const item_pocket *> wrapper_pockets = wrapper.get_pockets(
                    []( const item_pocket & pocket ) {
            return pocket.is_type( pocket_type::CONTAINER );
        } );
        if( wrapper_contents.size() != 1 || wrapper_pockets.size() != 1 ) {
            observation.valid_shapes = false;
            break;
        }
        const item &phone = **wrapper_contents.begin();
        if( phone.typeId() != itype_id( "smart_phone" ) &&
            phone.typeId() != itype_id( "smart_phone_locked" ) ) {
            observation.valid_shapes = false;
            break;
        }
        const bool first_phone_type = observation.phone_types.insert( phone.typeId().str() ).second;
        const std::list<const item *> efiles = phone.all_items_top( pocket_type::E_FILE_STORAGE );
        std::vector<std::string> efile_types;
        std::vector<int> efile_raw_damage;
        for( const item *efile : efiles ) {
            efile_types.push_back( efile->typeId().str() );
            efile_raw_damage.push_back( efile->damage() );
        }
        const bool first_empty = efiles.empty() && !observation.observed_empty_efiles;
        const bool first_many = efiles.size() >= 5 && !observation.observed_many_efiles;
        observation.observed_empty_efiles = observation.observed_empty_efiles || efiles.empty();
        observation.observed_many_efiles = observation.observed_many_efiles || efiles.size() >= 5;
        std::vector<std::string> witnesses;
        if( seed == 1 ) {
            witnesses.emplace_back( "fixed_seed:1" );
        }
        if( first_phone_type ) {
            witnesses.emplace_back( "first_phone_type:" + phone.typeId().str() );
        }
        if( first_empty ) {
            witnesses.emplace_back( "first_empty_efiles" );
        }
        if( first_many ) {
            witnesses.emplace_back( "first_five_or_more_efiles" );
        }
        const item_pocket &wrapper_pocket = *wrapper_pockets.front();
        const int downstream_draw = rng( 0, 9999 );
        for( const std::string &witness : witnesses ) {
            if( !retained_witnesses.insert( witness ).second ) {
                continue;
            }
            observation.exact_traces.push_back( {
                witness,
                seed,
                wrapper.typeId().str(),
                wrapper.has_itype_variant() ? wrapper.itype_variant().id : "",
                wrapper.any_pockets_sealed(),
                units::to_milliliter( wrapper_pocket.remaining_volume() ),
                units::to_gram( wrapper_pocket.remaining_weight() ),
                phone.typeId().str(),
                phone.charges,
                phone.ammo_remaining(),
                phone.ammo_current().str(),
                phone.damage(),
                efile_types,
                efile_raw_damage,
                downstream_draw
            } );
        }
        if( observation.phone_types.size() == 2 && observation.observed_empty_efiles &&
            observation.observed_many_efiles && retained_witnesses.size() == 5 ) {
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
    std::vector<tool_charge_trace> tool_charges;
    for( const int requested : { 0, 1, 56, 100 } ) {
        tool_charges.push_back( observe_tool_charges( requested ) );
    }
    const magazine_charge_observation magazine_charges = observe_magazine_charges();
    const repeated_tool_charge_trace repeated_tool_charges = observe_repeated_tool_charges();
    REQUIRE( repeated_tool_charges.seed > 0 );

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
    const description_expansion_trace description_expansion = observe_description_expansion();
    const variable_size_fit_observation variable_size_fit = observe_variable_size_fit();

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
    const std::vector<dressing_trace> direct_dressing = {
        observe_direct_dressing( "integral_full", "matches", 1, 100, 100 ),
        find_direct_dressing( "integral_chance_failure", "matches", 50, 100, 0 ),
        find_direct_dressing( "integral_chance_success", "matches", 50, 100, 20 ),
        observe_direct_dressing( "detachable_ammunition", "wearable_light", 1, 100, 0 ),
        observe_direct_dressing( "detachable_magazine", "wearable_light", 1, 0, 100 ),
        observe_direct_dressing( "explicit_charge_suppression", "permanent_marker", 1,
                                 100, 100, 0, 0 )
    };
    const std::vector<dressing_trace> production_dressing = {
        find_production_dressing( "lighter_matches_empty", "everyday_lighter", "matches", 0 ),
        find_production_dressing( "lighter_matches_full", "everyday_lighter", "matches", 20 ),
        find_production_dressing( "lighter_ref_matches_empty", "everyday_lighter", "ref_matches", 0 ),
        find_production_dressing( "lighter_ref_matches_full", "everyday_lighter", "ref_matches", 32 ),
        find_production_dressing( "gear_marker_empty", "everyday_gear", "permanent_marker", 0 ),
        find_production_dressing( "gear_marker_full", "everyday_gear", "permanent_marker", 500 ),
        find_production_dressing( "gear_inhaler_minimum", "everyday_gear", "inhaler", 10 ),
        find_production_dressing( "gear_inhaler_maximum", "everyday_gear", "inhaler", 100 ),
        find_production_dressing( "gear_flashlight_empty", "everyday_gear", "flashlight", 0 ),
        find_production_dressing( "gear_flashlight_full", "everyday_gear", "flashlight", 56 )
    };

    const container_observation discarded = observe_container_group(
                "discard", item_group_id( "test_truncating_to_container" ) );
    const container_observation spilled = observe_container_group(
                "spill", item_group_id( "test_spilling_from_container" ) );
    const std::vector<multi_pocket_trace> multi_pocket_wrappers = {
        observe_multi_pocket_wrapper( "leg_sheath_minimum", item_group_id( "costume_accessories" ),
                                      itype_id( "leg_sheath6" ), itype_id( "throwing_knife" ), 1 ),
        observe_multi_pocket_wrapper( "leg_sheath_maximum", item_group_id( "costume_accessories" ),
                                      itype_id( "leg_sheath6" ), itype_id( "throwing_knife" ), 6 ),
        observe_multi_pocket_wrapper( "hard_hat_mandible", item_group_id( "costume_hats_hoods" ),
                                      itype_id( "hat_hard" ),
                                      itype_id( "plastic_mandible_guard" ), 1 )
    };
    REQUIRE( multi_pocket_wrappers[0].pocket_contents.size() == 6 );
    REQUIRE( multi_pocket_wrappers[0].pocket_contents[0].size() == 1 );
    REQUIRE( multi_pocket_wrappers[1].pocket_contents ==
             std::vector<std::vector<std::string>>( 6,
                     std::vector<std::string>{ "throwing_knife" } ) );
    REQUIRE( multi_pocket_wrappers[2].pocket_contents.size() == 6 );
    REQUIRE( multi_pocket_wrappers[2].pocket_contents[3] ==
             std::vector<std::string>{ "plastic_mandible_guard" } );
    const std::vector<named_snippet_category_trace> named_snippet_categories = {
        observe_named_snippet_category( "months_old_news", itype_id( "months_old_newspaper" ),
                                        "months_old_news" ),
        observe_named_snippet_category( "wallet_photos", itype_id( "wallet_photo" ),
                                        "wallet_photos" )
    };
    REQUIRE( named_snippet_categories[0].choice_ids.size() == 24 );
    REQUIRE( named_snippet_categories[0].choice_ids.front() == "months_old_news_1" );
    REQUIRE( named_snippet_categories[0].choice_ids.back() == "months_old_news_25" );
    REQUIRE( named_snippet_categories[1].choice_ids.size() == 38 );
    REQUIRE( named_snippet_categories[1].choice_ids.front() == "wallet_picture_1" );
    REQUIRE( named_snippet_categories[1].choice_ids.back() == "wallet_picture_38" );
    const corpse_observation corpses = observe_everyday_corpses();
    const phone_case_observation phone_cases = observe_civilian_phone_cases();
    REQUIRE( phone_cases.valid_shapes );
    REQUIRE( phone_cases.phone_types == std::set<std::string>{ "smart_phone", "smart_phone_locked" } );
    REQUIRE( phone_cases.observed_empty_efiles );
    REQUIRE( phone_cases.observed_many_efiles );

    constexpr unsigned int modifier_container_capacity_seed = 31415;
    const modifier_container_capacity_trace explicit_container_capacity =
        observe_modifier_container_capacity( modifier_container_capacity_seed, 50, 80 );
    const modifier_container_capacity_trace default_container_capacity =
        observe_modifier_container_capacity( modifier_container_capacity_seed, -1, -1 );
    const modifier_container_capacity_trace fixed_container_capacity =
        observe_modifier_container_capacity( modifier_container_capacity_seed,
                explicit_container_capacity.payload_charges,
                explicit_container_capacity.payload_charges );
    REQUIRE( explicit_container_capacity.container_type == "bottle_plastic" );
    REQUIRE( explicit_container_capacity.payload_type == "water_clean" );
    REQUIRE( explicit_container_capacity.payload_charges ==
             default_container_capacity.payload_charges );
    REQUIRE( explicit_container_capacity.downstream_draw ==
             fixed_container_capacity.downstream_draw );

    const std::vector<charge_capacity_sentinel_trace> charge_capacity_sentinels = {
        observe_charge_capacity_sentinel( "integral_tool_minimum", "eink_tablet_pc", 78,
                                          0, -1 ),
        observe_charge_capacity_sentinel( "integral_tool_maximum", "eink_tablet_pc", 31415,
                                          0, -1 ),
        observe_charge_capacity_sentinel( "ordinary_unresolved", "rock", 31415,
                                          4, -1 ),
        find_charge_capacity_sentinel( "detachable_tool_minimum", "wearable_light",
                                       0, -1, 0 ),
        find_charge_capacity_sentinel( "detachable_tool_maximum", "wearable_light",
                                       0, -1, 56 ),
        observe_charge_capacity_sentinel( "detachable_explicit_over_capacity", "wearable_light",
                                          31415, 0, 100 ),
        find_charge_capacity_sentinel( "magazine_minimum", "light_battery_cell",
                                       0, -1, 0 ),
        find_charge_capacity_sentinel( "magazine_maximum", "light_battery_cell",
                                       0, -1, 16 ),
        find_charge_capacity_sentinel( "container_minimum", "water_clean",
                                       1, -1, 1, "bottle_plastic", 1 ),
        find_charge_capacity_sentinel( "container_maximum", "water_clean",
                                       1, -1, 2, "bottle_plastic", 2 ),
        find_charge_capacity_sentinel( "lower_sentinel_minimum", "40x46mm_m1006",
                                       -1, 4, 1, "", 1 ),
        find_charge_capacity_sentinel( "lower_sentinel_maximum", "40x46mm_m1006",
                                       -1, 4, 4, "", 4 )
    };
    REQUIRE( charge_capacity_sentinels[0].ammunition_remaining == 0 );
    REQUIRE( charge_capacity_sentinels[0].remaining_capacity == 85 );
    REQUIRE( charge_capacity_sentinels[1].item_charges == 0 );
    REQUIRE( charge_capacity_sentinels[1].ammunition_type == "battery" );
    REQUIRE( charge_capacity_sentinels[1].ammunition_remaining == 85 );
    REQUIRE_FALSE( charge_capacity_sentinels[1].magazine_present );
    REQUIRE( charge_capacity_sentinels[1].remaining_capacity == 0 );
    REQUIRE( charge_capacity_sentinels[2].item_charges == 1 );
    REQUIRE( charge_capacity_sentinels[2].ammunition_remaining == 1 );
    REQUIRE_FALSE( charge_capacity_sentinels[2].magazine_present );
    REQUIRE( charge_capacity_sentinels[3].magazine_present );
    REQUIRE( charge_capacity_sentinels[4].magazine_present );
    REQUIRE( charge_capacity_sentinels[4].magazine_type == "medium_battery_cell" );
    REQUIRE( charge_capacity_sentinels[5].effective_maximum == 100 );
    REQUIRE( charge_capacity_sentinels[5].magazine_present );
    REQUIRE( charge_capacity_sentinels[6].ammunition_type == "null" );
    REQUIRE( charge_capacity_sentinels[7].ammunition_type == "battery" );
    REQUIRE( charge_capacity_sentinels[8].wrapper_type == "bottle_plastic" );
    REQUIRE( charge_capacity_sentinels[9].wrapper_type == "bottle_plastic" );

    const std::vector<default_container_trace> default_containers = {
        observe_default_container( "direct_water", "water_clean", 31415,
                                   default_container_mode::unmodified ),
        observe_default_container( "direct_aspirin", "aspirin", 31415,
                                   default_container_mode::unmodified ),
        observe_default_container( "modifier_aspirin", "aspirin", 31415,
                                   default_container_mode::modifier_fallback ),
        observe_default_container( "suppressed_aspirin", "aspirin", 31415,
                                   default_container_mode::modifier_suppressed ),
        observe_default_container( "explicit_container_default", "ibuprofen", 31415,
                                   default_container_mode::explicit_container_default ),
        observe_painkiller_group_boundary( "production_aspirin_minimum", 1 ),
        observe_painkiller_group_boundary( "production_aspirin_maximum", 20 )
    };
    const std::vector<flexible_wrapper_trace> flexible_wrappers = {
        observe_chaw_wrapper_boundary( "production_chaw_minimum", 1 ),
        observe_chaw_wrapper_boundary( "production_chaw_maximum", 20 ),
        observe_chewing_gum_wrapper()
    };
    REQUIRE( flexible_wrappers[0].outer_type == "wrapper" );
    REQUIRE_FALSE( flexible_wrappers[0].pocket_rigid );
    REQUIRE_FALSE( flexible_wrappers[0].pocket_collapsed_by_default );
    REQUIRE( flexible_wrappers[0].pocket_collapsed );
    REQUIRE( flexible_wrappers[0].content_types == std::vector<std::string>{ "chaw" } );
    REQUIRE( flexible_wrappers[0].content_charges == std::vector<int>{ 0 } );
    REQUIRE( flexible_wrappers[0].outer_volume_ml == 50 );
    REQUIRE( flexible_wrappers[0].pocket_remaining_volume_ml <
             flexible_wrappers[0].pocket_capacity_volume_ml );
    REQUIRE( flexible_wrappers[1].content_types == std::vector<std::string>( 20, "chaw" ) );
    REQUIRE( flexible_wrappers[1].content_charges == std::vector<int>( 20, 0 ) );
    REQUIRE( flexible_wrappers[1].outer_volume_ml > flexible_wrappers[0].outer_volume_ml );
    REQUIRE( flexible_wrappers[1].outer_weight_g > flexible_wrappers[0].outer_weight_g );
    REQUIRE( flexible_wrappers[1].pocket_remaining_volume_ml <
             flexible_wrappers[0].pocket_remaining_volume_ml );
    REQUIRE( flexible_wrappers[2].outer_variant == "blister_pack_gum" );
    REQUIRE( flexible_wrappers[2].pocket_collapsed_by_default );
    REQUIRE( flexible_wrappers[2].pocket_collapsed );
    REQUIRE( flexible_wrappers[2].content_types == std::vector<std::string>( 12, "gum" ) );
    REQUIRE( std::all_of( flexible_wrappers[2].content_variants.begin(),
                         flexible_wrappers[2].content_variants.end(), []( const std::string & variant ) {
        return !variant.empty();
    } ) );
    const std::vector<temperature_constructor_trace> temperature_constructors = {
        observe_temperature_constructor( "materialless_comestible", "chaw", 123 ),
        observe_temperature_constructor( "material_comestible", "water_clean", 123 ),
        observe_temperature_constructor( "field_blocker_material", "caff_gum", 123 ),
        observe_temperature_constructor( "weighted_material", "saline", 123 ),
        observe_temperature_constructor( "custom_freezing_comestible", "whiskey", 123 ),
        observe_temperature_constructor( "never_freeze_sentinel", "powder_eggs", 123 ),
        observe_temperature_constructor( "positive_freezing_comestible", "chem_benzene", 123 ),
        observe_temperature_constructor( "no_temp_comestible", "caffeine", 123 ),
        observe_temperature_constructor( "ordinary_control", "rock", 123 )
    };
    REQUIRE( temperature_constructors[0].has_temperature );
    REQUIRE( temperature_constructors[0].active );
    REQUIRE( temperature_constructors[0].processing_speed == to_turns<int>( 10_minutes ) );
    REQUIRE( temperature_constructors[0].temperature_millikelvin == 0 );
    REQUIRE( temperature_constructors[0].specific_energy_millijoules_per_gram == -10000 );
    REQUIRE( temperature_constructors[0].serialized_last_temp_check_present );
    REQUIRE( temperature_constructors[0].solid );
    REQUIRE_FALSE( temperature_constructors[0].liquid );
    REQUIRE( temperature_constructors[1].has_temperature );
    REQUIRE( temperature_constructors[1].active );
    REQUIRE( temperature_constructors[1].liquid );
    REQUIRE( temperature_constructors[1].thermal_properties_present );
    REQUIRE( temperature_constructors[2].specific_heat_liquid_microjoules_per_gram_kelvin ==
             1500000 );
    REQUIRE( temperature_constructors[2].specific_heat_solid_microjoules_per_gram_kelvin ==
             1200000 );
    REQUIRE( temperature_constructors[2].latent_heat_microjoules_per_gram == 10000000 );
    REQUIRE( temperature_constructors[2].ambient_specific_energy_millijoules_per_gram == 367780 );
    REQUIRE( temperature_constructors[3].thermal_properties_present );
    REQUIRE( temperature_constructors[3].liquid );
    REQUIRE( temperature_constructors[4].thermal_properties_present );
    REQUIRE( temperature_constructors[4].freezing_point_millikelvin == 243150 );
    REQUIRE( temperature_constructors[4].ambient_specific_energy_millijoules_per_gram == 996300 );
    REQUIRE( temperature_constructors[4].liquid );
    REQUIRE( temperature_constructors[5].freezing_point_millikelvin == -850 );
    REQUIRE( temperature_constructors[5].solid );
    REQUIRE( temperature_constructors[6].freezing_point_millikelvin == 278150 );
    REQUIRE( temperature_constructors[6].liquid );
    REQUIRE_FALSE( temperature_constructors[7].has_temperature );
    REQUIRE_FALSE( temperature_constructors[7].active );
    REQUIRE_FALSE( temperature_constructors[8].has_temperature );
    REQUIRE_FALSE( temperature_constructors[8].active );
    REQUIRE( default_containers[0].outer_type == "bottle_plastic" );
    REQUIRE( default_containers[0].content_types == std::vector<std::string>{ "water_clean" } );
    REQUIRE( default_containers[0].payload_charges == 2 );
    REQUIRE( default_containers[0].sealed );
    REQUIRE( default_containers[1].outer_type == "bottle_plastic_pill_painkiller" );
    REQUIRE( default_containers[1].content_types == std::vector<std::string>{ "aspirin" } );
    REQUIRE( default_containers[2].outer_type == "bottle_plastic_pill_painkiller" );
    REQUIRE( default_containers[2].content_types == std::vector<std::string>{ "aspirin" } );
    REQUIRE( default_containers[3].outer_type == "aspirin" );
    REQUIRE( default_containers[3].content_types.empty() );
    REQUIRE( default_containers[4].outer_type == "bottle_plastic_pill_painkiller" );
    REQUIRE( default_containers[4].content_types ==
             std::vector<std::string>{ "ibuprofen", "aspirin" } );
    REQUIRE( default_containers[5].content_types.size() == 1 );
    REQUIRE( default_containers[6].content_types.size() == 20 );

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

        json.member( "tool_charges" );
        json.start_array();
        for( const tool_charge_trace &trace : tool_charges ) {
            json.start_object();
            json.member( "requested_charges", trace.requested_charges );
            json.member( "tool_type", trace.tool_type );
            json.member( "magazine_present", trace.magazine_present );
            json.member( "magazine_type", trace.magazine_type );
            json.member( "ammunition_type", trace.ammunition_type );
            json.member( "ammunition_remaining", trace.ammunition_remaining );
            json.member( "remaining_capacity", trace.remaining_capacity );
            json.end_object();
        }
        json.end_array();

        const auto write_magazine_charge_traces = [&]( const std::vector<magazine_charge_trace> &traces ) {
            json.start_array();
            for( const magazine_charge_trace &trace : traces ) {
                json.start_object();
                json.member( "case_id", trace.case_id );
                json.member( "seed", trace.seed );
                json.member( "requested_charges", trace.requested_charges );
                json.member( "item_type", trace.item_type );
                json.member( "ammunition_type", trace.ammunition_type );
                json.member( "ammunition_remaining", trace.ammunition_remaining );
                json.member( "remaining_capacity", trace.remaining_capacity );
                json.member( "downstream_draw", trace.downstream_draw );
                json.end_object();
            }
            json.end_array();
        };
        json.member( "magazine_charges" );
        json.start_object();
        json.member( "production_group", magazine_charges.production_group );
        json.member( "direct" );
        write_magazine_charge_traces( magazine_charges.direct );
        json.member( "production" );
        write_magazine_charge_traces( magazine_charges.production );
        json.end_object();

        json.member( "repeated_tool_charges" );
        json.start_object();
        json.member( "source_group", repeated_tool_charges.source_group );
        json.member( "seed", repeated_tool_charges.seed );
        json.member( "leaf_minimum", repeated_tool_charges.leaf_minimum );
        json.member( "leaf_maximum", repeated_tool_charges.leaf_maximum );
        json.member( "replacement_requested", repeated_tool_charges.replacement_requested );
        json.member( "tool_type", repeated_tool_charges.tool_type );
        json.member( "magazine_type", repeated_tool_charges.magazine_type );
        json.member( "ammunition_type", repeated_tool_charges.ammunition_type );
        json.member( "ammunition_remaining", repeated_tool_charges.ammunition_remaining );
        json.member( "downstream_draw", repeated_tool_charges.downstream_draw );
        json.end_object();

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

        json.member( "description_expansion" );
        json.start_object();
        json.member( "direct_input", description_expansion.direct_input );
        json.member( "direct_output", description_expansion.direct_output );
        json.member( "direct_downstream_draw", description_expansion.direct_downstream_draw );
        json.member( "source_group", description_expansion.source_group );
        json.member( "seed", description_expansion.seed );
        json.member( "item_type", description_expansion.item_type );
        json.member( "variant_id", description_expansion.variant_id );
        json.member( "expanded_description", description_expansion.expanded_description );
        json.member( "downstream_draw", description_expansion.downstream_draw );
        json.end_object();

        const auto write_fit_traces = [&]( const std::vector<variable_size_fit_trace> &traces ) {
            json.start_array();
            for( const variable_size_fit_trace &trace : traces ) {
                json.start_object();
                json.member( "case_id", trace.case_id );
                json.member( "seed", trace.seed );
                json.member( "item_type", trace.item_type );
                json.member( "variable_size", trace.variable_size );
                json.member( "fitted", trace.fitted );
                json.member( "name", trace.name );
                json.member( "downstream_draw", trace.downstream_draw );
                json.end_object();
            }
            json.end_array();
        };
        json.member( "variable_size_fit" );
        json.start_object();
        json.member( "production_group", variable_size_fit.production_group );
        json.member( "direct" );
        write_fit_traces( variable_size_fit.direct );
        json.member( "production" );
        write_fit_traces( variable_size_fit.production );
        json.end_object();

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

        const auto write_dressing_traces = [&]( const std::vector<dressing_trace> &traces ) {
            json.start_array();
            for( const dressing_trace &trace : traces ) {
                json.start_object();
                json.member( "case_id", trace.case_id );
                json.member( "source_group", trace.source_group );
                json.member( "seed", trace.seed );
                json.member( "ammunition_chance", trace.ammunition_chance );
                json.member( "magazine_chance", trace.magazine_chance );
                json.member( "charges_minimum", trace.charges_minimum );
                json.member( "charges_maximum", trace.charges_maximum );
                json.member( "item_type", trace.item_type );
                json.member( "magazine_present", trace.magazine_present );
                json.member( "magazine_type", trace.magazine_type );
                json.member( "ammunition_type", trace.ammunition_type );
                json.member( "ammunition_remaining", trace.ammunition_remaining );
                json.member( "remaining_capacity", trace.remaining_capacity );
                json.member( "downstream_draw", trace.downstream_draw );
                json.end_object();
            }
            json.end_array();
        };
        json.member( "dressing" );
        json.start_object();
        json.member( "direct" );
        write_dressing_traces( direct_dressing );
        json.member( "production" );
        write_dressing_traces( production_dressing );
        json.end_object();

        json.member( "modifier_container_capacity" );
        json.start_object();
        json.member( "seed", explicit_container_capacity.seed );
        json.member( "container_type", explicit_container_capacity.container_type );
        json.member( "payload_type", explicit_container_capacity.payload_type );
        json.member( "explicit_minimum", 50 );
        json.member( "explicit_maximum", 80 );
        json.member( "explicit_charges", explicit_container_capacity.payload_charges );
        json.member( "default_charges", default_container_capacity.payload_charges );
        json.member( "explicit_downstream_draw", explicit_container_capacity.downstream_draw );
        json.member( "fixed_downstream_draw", fixed_container_capacity.downstream_draw );
        json.member( "downstream_draw_matches",
                     explicit_container_capacity.downstream_draw ==
                     fixed_container_capacity.downstream_draw );
        json.end_object();

        json.member( "charge_capacity_sentinels" );
        json.start_array();
        for( const charge_capacity_sentinel_trace &trace : charge_capacity_sentinels ) {
            json.start_object();
            json.member( "case_id", trace.case_id );
            json.member( "seed", trace.seed );
            json.member( "minimum", trace.minimum );
            json.member( "maximum", trace.maximum );
            json.member( "effective_minimum", trace.effective_minimum );
            json.member( "effective_maximum", trace.effective_maximum );
            json.member( "item_type", trace.item_type );
            json.member( "item_charges", trace.item_charges );
            json.member( "ammunition_type", trace.ammunition_type );
            json.member( "ammunition_remaining", trace.ammunition_remaining );
            json.member( "remaining_capacity", trace.remaining_capacity );
            json.member( "magazine_present", trace.magazine_present );
            json.member( "magazine_type", trace.magazine_type );
            json.member( "wrapper_type", trace.wrapper_type );
            json.member( "downstream_draw", trace.downstream_draw );
            json.end_object();
        }
        json.end_array();

        json.member( "default_containers" );
        json.start_array();
        for( const default_container_trace &trace : default_containers ) {
            json.start_object();
            json.member( "case_id", trace.case_id );
            json.member( "seed", trace.seed );
            json.member( "outer_type", trace.outer_type );
            json.member( "content_types" );
            write_trace( json, trace.content_types );
            json.member( "payload_charges", trace.payload_charges );
            json.member( "sealed", trace.sealed );
            json.member( "pocket_collapsed", trace.pocket_collapsed );
            json.member( "downstream_draw", trace.downstream_draw );
            json.end_object();
        }
        json.end_array();

        json.member( "flexible_wrappers" );
        json.start_array();
        for( const flexible_wrapper_trace &trace : flexible_wrappers ) {
            json.start_object();
            json.member( "case_id", trace.case_id );
            json.member( "seed", trace.seed );
            json.member( "outer_type", trace.outer_type );
            json.member( "outer_variant", trace.outer_variant );
            json.member( "pocket_rigid", trace.pocket_rigid );
            json.member( "pocket_collapsed_by_default", trace.pocket_collapsed_by_default );
            json.member( "pocket_collapsed", trace.pocket_collapsed );
            json.member( "content_types" );
            write_trace( json, trace.content_types );
            json.member( "content_variants" );
            write_trace( json, trace.content_variants );
            json.member( "content_charges" );
            json.start_array();
            for( const int charges : trace.content_charges ) {
                json.write( charges );
            }
            json.end_array();
            json.member( "outer_volume_ml", trace.outer_volume_ml );
            json.member( "outer_weight_g", trace.outer_weight_g );
            json.member( "pocket_capacity_volume_ml", trace.pocket_capacity_volume_ml );
            json.member( "pocket_remaining_volume_ml", trace.pocket_remaining_volume_ml );
            json.member( "pocket_remaining_weight_g", trace.pocket_remaining_weight_g );
            json.member( "sealed", trace.sealed );
            json.member( "downstream_draw", trace.downstream_draw );
            json.end_object();
        }
        json.end_array();

        json.member( "temperature_constructors" );
        json.start_array();
        for( const temperature_constructor_trace &trace : temperature_constructors ) {
            json.start_object();
            json.member( "case_id", trace.case_id );
            json.member( "item_type", trace.item_type );
            json.member( "birth_turn", trace.birth_turn );
            json.member( "has_temperature", trace.has_temperature );
            json.member( "active", trace.active );
            json.member( "processing_speed", trace.processing_speed );
            json.member( "temperature_millikelvin", trace.temperature_millikelvin );
            json.member( "specific_energy_millijoules_per_gram",
                         trace.specific_energy_millijoules_per_gram );
            json.member( "thermal_properties_present", trace.thermal_properties_present );
            json.member( "specific_heat_liquid_microjoules_per_gram_kelvin",
                         trace.specific_heat_liquid_microjoules_per_gram_kelvin );
            json.member( "specific_heat_solid_microjoules_per_gram_kelvin",
                         trace.specific_heat_solid_microjoules_per_gram_kelvin );
            json.member( "latent_heat_microjoules_per_gram",
                         trace.latent_heat_microjoules_per_gram );
            json.member( "freezing_point_millikelvin", trace.freezing_point_millikelvin );
            json.member( "ambient_specific_energy_millijoules_per_gram",
                         trace.ambient_specific_energy_millijoules_per_gram );
            json.member( "serialized_last_temp_check_present",
                         trace.serialized_last_temp_check_present );
            json.member( "serialized_last_temp_check", trace.serialized_last_temp_check );
            json.member( "solid", trace.solid );
            json.member( "liquid", trace.liquid );
            json.member( "hot", trace.hot );
            json.member( "cold", trace.cold );
            json.member( "frozen", trace.frozen );
            json.end_object();
        }
        json.end_array();

        json.member( "named_snippet_categories" );
        json.start_array();
        for( const named_snippet_category_trace &trace : named_snippet_categories ) {
            json.start_object();
            json.member( "case_id", trace.case_id );
            json.member( "item_type", trace.item_type );
            json.member( "category", trace.category );
            json.member( "choice_ids" );
            json.start_array();
            for( const std::string &id : trace.choice_ids ) {
                json.write( id );
            }
            json.end_array();
            json.member( "first_text", trace.first_text );
            json.member( "last_text", trace.last_text );
            const auto write_selection = [&]( const char *member,
            const named_snippet_selection_trace & selection ) {
                json.member( member );
                json.start_object();
                json.member( "seed", selection.seed );
                json.member( "snippet_id", selection.snippet_id );
                json.member( "text", selection.text );
                json.member( "downstream_draw", selection.downstream_draw );
                json.end_object();
            };
            write_selection( "first_selection", trace.first_selection );
            write_selection( "last_selection", trace.last_selection );
            json.end_object();
        }
        json.end_array();

        json.member( "multi_pocket_wrappers" );
        json.start_array();
        for( const multi_pocket_trace &trace : multi_pocket_wrappers ) {
            json.start_object();
            json.member( "case_id", trace.case_id );
            json.member( "seed", trace.seed );
            json.member( "wrapper_type", trace.wrapper_type );
            json.member( "payload_type", trace.payload_type );
            json.member( "pocket_contents" );
            json.start_array();
            for( const std::vector<std::string> &contents : trace.pocket_contents ) {
                write_trace( json, contents );
            }
            json.end_array();
            json.member( "downstream_draw", trace.downstream_draw );
            json.end_object();
        }
        json.end_array();

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

        json.member( "civilian_phone_case" );
        json.start_object();
        json.member( "seed_search_limit", phone_cases.seed_search_limit );
        json.member( "valid_shapes", phone_cases.valid_shapes );
        json.member( "phone_types" );
        write_strings( json, phone_cases.phone_types );
        json.member( "observed_empty_efiles", phone_cases.observed_empty_efiles );
        json.member( "observed_many_efiles", phone_cases.observed_many_efiles );
        json.member( "exact_traces" );
        json.start_array();
        for( const phone_case_observation::exact_trace &trace : phone_cases.exact_traces ) {
            json.start_object();
            json.member( "witness", trace.witness );
            json.member( "seed", trace.seed );
            json.member( "wrapper_type", trace.wrapper_type );
            json.member( "wrapper_variant", trace.wrapper_variant );
            json.member( "wrapper_any_pocket_sealed", trace.wrapper_any_pocket_sealed );
            json.member( "wrapper_remaining_volume_ml", trace.wrapper_remaining_volume_ml );
            json.member( "wrapper_remaining_weight_g", trace.wrapper_remaining_weight_g );
            json.member( "phone_type", trace.phone_type );
            json.member( "phone_charges", trace.phone_charges );
            json.member( "phone_ammo_remaining", trace.phone_ammo_remaining );
            json.member( "phone_ammunition_type", trace.phone_ammunition_type );
            json.member( "phone_raw_damage", trace.phone_raw_damage );
            json.member( "efile_types" );
            write_trace( json, trace.efile_types );
            json.member( "efile_raw_damage" );
            json.start_array();
            for( const int damage : trace.efile_raw_damage ) {
                json.write( damage );
            }
            json.end_array();
            json.member( "downstream_draw", trace.downstream_draw );
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
