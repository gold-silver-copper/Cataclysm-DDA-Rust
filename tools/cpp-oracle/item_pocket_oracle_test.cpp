#include <cstdlib>
#include <fstream>
#include <string>

#include "cata_catch.h"
#include "item.h"
#include "item_pocket.h"
#include "json.h"
#include "pocket_type.h"
#include "type_id.h"
#include "units.h"

namespace
{

constexpr const char *baseline_commit = "4dfd36038b16650dc1b5cb9d79a3e42363174b05";
constexpr const char *upstream_tree = "210f31db2e8b2f0caed1809f1a66781859f9d129";

const char *contain_code_name( item_pocket::contain_code code )
{
    switch( code ) {
        case item_pocket::contain_code::SUCCESS:
            return "success";
        case item_pocket::contain_code::ERR_MOD:
            return "err_mod";
        case item_pocket::contain_code::ERR_LIQUID:
            return "err_liquid";
        case item_pocket::contain_code::ERR_GAS:
            return "err_gas";
        case item_pocket::contain_code::ERR_TOO_BIG:
            return "err_too_big";
        case item_pocket::contain_code::ERR_TOO_HEAVY:
            return "err_too_heavy";
        case item_pocket::contain_code::ERR_TOO_SMALL:
            return "err_too_small";
        case item_pocket::contain_code::ERR_NO_SPACE:
            return "err_no_space";
        case item_pocket::contain_code::ERR_CANNOT_SUPPORT:
            return "err_cannot_support";
        case item_pocket::contain_code::ERR_FLAG:
            return "err_flag";
        case item_pocket::contain_code::ERR_AMMO:
            return "err_ammo";
    }
    return "unknown";
}

struct oracle_case {
    const char *case_id;
    const char *item_id;
};

} // namespace

TEST_CASE( "rust_cpp_oracle_item_pocket_max_length", "[cpp-oracle][pocket]" )
{
    const char *output_path = std::getenv( "CDDA_RUST_CPP_ORACLE_OUTPUT" );
    REQUIRE( output_path != nullptr );
    REQUIRE( std::string( output_path ).size() > 0 );

    const itype_id sonic_id( "test_sonic_screwdriver" );
    REQUIRE( sonic_id.is_valid() );
    const item sonic( sonic_id );

    pocket_data pocket_definition( pocket_type::CONTAINER );
    pocket_definition.raw_volume_capacity = 10_liter;
    pocket_definition.max_contains_weight = 10_kilogram;
    pocket_definition.max_item_length = sonic.length();
    const item_pocket pocket( &pocket_definition );

    const oracle_case cases[] = {
        { "shorter", "test_screwdriver" },
        { "equal", "test_sonic_screwdriver" },
        { "longer", "test_clumsy_sword" },
    };

    std::ofstream output( output_path, std::ios::out | std::ios::trunc );
    REQUIRE( output.is_open() );
    {
        JsonOut json( output );
        json.start_object();
        json.member( "format_version", 1 );
        json.member( "baseline_commit", baseline_commit );
        json.member( "upstream_tree", upstream_tree );
        json.member( "kernel", "item_pocket_max_length_v1" );
        json.member( "pocket" );
        json.start_object();
        json.member( "pocket_type", "CONTAINER" );
        json.member( "max_item_length_mm", units::to_millimeter( pocket_definition.max_item_length ) );
        json.member( "volume_capacity_ml", units::to_milliliter( pocket_definition.volume_capacity() ) );
        json.member( "weight_capacity_g", units::to_gram( pocket_definition.max_contains_weight ) );
        json.end_object();
        json.member( "cases" );
        json.start_array();
        for( const oracle_case &definition : cases ) {
            const itype_id item_id( definition.item_id );
            REQUIRE( item_id.is_valid() );
            const item candidate( item_id );
            const ret_val<item_pocket::contain_code> result = pocket.can_contain( candidate );
            json.start_object();
            json.member( "case_id", definition.case_id );
            json.member( "item_id", definition.item_id );
            json.member( "item_length_mm", units::to_millimeter( candidate.length() ) );
            json.member( "success", result.success() );
            json.member( "contain_code", static_cast<int>( result.value() ) );
            json.member( "contain_code_name", contain_code_name( result.value() ) );
            json.member( "reason", result.str() );
            json.end_object();
        }
        json.end_array();
        json.end_object();
    }
    output.close();
    REQUIRE( output.good() );
}
