#include <algorithm>
#include <cstdlib>
#include <fstream>
#include <string>
#include <utility>
#include <vector>

#include "cata_catch.h"
#include "json.h"
#include "json_loader.h"
#include "mapgen.h"
#include "omdata.h"
#include "overmap.h"
#include "point.h"
#include "type_id.h"

namespace
{

constexpr const char *baseline_commit = "4dfd36038b16650dc1b5cb9d79a3e42363174b05";
constexpr const char *upstream_tree = "210f31db2e8b2f0caed1809f1a66781859f9d129";

struct match_case {
    const char *case_id;
    const char *query;
    const char *terrain_id;
    const char *match_type;
    ot_match_type type;
};

struct rotation_observation {
    std::string direction;
    std::string terrain_id;
    std::string mapgen_id;
    int rotation;
    point marker;
};

const char *phase_name( mapgen_phase phase )
{
    switch( phase ) {
        case mapgen_phase::removal:
            return "removal";
        case mapgen_phase::terrain:
            return "terrain";
        case mapgen_phase::furniture:
            return "furniture";
        case mapgen_phase::default_:
            return "default";
        case mapgen_phase::nested_mapgen:
            return "nested_mapgen";
        case mapgen_phase::transform:
            return "transform";
        case mapgen_phase::faction_ownership:
            return "faction_ownership";
        case mapgen_phase::zones:
            return "zones";
        case mapgen_phase::last:
            break;
    }
    return "invalid";
}

std::vector<rotation_observation> rotatable_observations()
{
    const oter_type_str_id shelter( "shelter" );
    REQUIRE( shelter.is_valid() );
    constexpr point marker( 2, 5 );
    constexpr point dimensions( 24, 24 );
    const std::pair<om_direction::type, const char *> directions[] = {
        { om_direction::type::north, "north" },
        { om_direction::type::east, "east" },
        { om_direction::type::south, "south" },
        { om_direction::type::west, "west" },
    };
    std::vector<rotation_observation> result;
    for( const auto &[direction, name] : directions ) {
        const oter_id terrain = shelter->get_rotated( direction );
        result.push_back( { name, terrain.id().str(), terrain->get_mapgen_id(),
                            terrain->get_rotation(), marker.rotate( static_cast<int>( direction ), dimensions ) } );
    }
    return result;
}

std::vector<rotation_observation> linear_observations()
{
    const oter_str_id road_ns( "road_ns" );
    REQUIRE( road_ns.is_valid() );
    constexpr point marker( 2, 5 );
    constexpr point dimensions( 24, 24 );
    const std::pair<om_direction::type, const char *> directions[] = {
        { om_direction::type::north, "north" },
        { om_direction::type::east, "east" },
        { om_direction::type::south, "south" },
        { om_direction::type::west, "west" },
    };
    std::vector<rotation_observation> result;
    for( const auto &[direction, name] : directions ) {
        const oter_id terrain = road_ns->get_rotated( direction );
        result.push_back( { name, terrain.id().str(), terrain->get_mapgen_id(),
                            terrain->get_rotation(), marker.rotate( static_cast<int>( direction ), dimensions ) } );
    }
    return result;
}

void write_rotations( JsonOut &json, const std::vector<rotation_observation> &observations )
{
    json.start_array();
    for( const rotation_observation &observation : observations ) {
        json.start_object();
        json.member( "direction", observation.direction );
        json.member( "terrain_id", observation.terrain_id );
        json.member( "mapgen_id", observation.mapgen_id );
        json.member( "rotation", observation.rotation );
        json.member( "marker_x", observation.marker.x );
        json.member( "marker_y", observation.marker.y );
        json.end_object();
    }
    json.end_array();
}

} // namespace

TEST_CASE( "rust_cpp_oracle_mapgen_static_semantics", "[cpp-oracle][mapgen]" )
{
    const char *output_path = std::getenv( "CDDA_RUST_CPP_ORACLE_OUTPUT" );
    REQUIRE( output_path != nullptr );
    REQUIRE( std::string( output_path ).size() > 0 );

    const match_case matching[] = {
        { "exact_full", "shelter_north", "shelter_north", "EXACT", ot_match_type::exact },
        { "exact_base_rejected", "shelter", "shelter_north", "EXACT", ot_match_type::exact },
        { "rotatable_type", "shelter", "shelter_east", "TYPE", ot_match_type::type },
        { "linear_subtype", "road_straight", "road_ew", "SUBTYPE", ot_match_type::subtype },
        { "wrong_linear_subtype", "road_curved", "road_ew", "SUBTYPE", ot_match_type::subtype },
        { "prefix_separator", "forest", "forest_thick", "PREFIX", ot_match_type::prefix },
        { "partial_prefix_rejected", "fore", "forest_thick", "PREFIX", ot_match_type::prefix },
        { "contains_substring", "rest_t", "forest_thick", "CONTAINS", ot_match_type::contains },
    };
    for( const match_case &definition : matching ) {
        REQUIRE( oter_str_id( definition.terrain_id ).is_valid() );
    }

    const std::vector<rotation_observation> rotatable = rotatable_observations();
    const std::vector<rotation_observation> linear = linear_observations();

    constexpr const char *palette_id_text = "rust_cpp_oracle_mapgen_palette_v1";
    JsonObject palette_json = json_loader::from_string( R"({
        "id": "rust_cpp_oracle_mapgen_palette_v1",
        "terrain": { "X": "t_floor" },
        "furniture": { "X": "f_table" },
        "remove_all": { "X": {} },
        "nested": { "X": { "chunks": [ "null" ] } }
    })" ).get_object();
    mapgen_palette::load( palette_json, "dda" );
    const mapgen_palette &palette = mapgen_palette::get( palette_id( palette_id_text ) );
    const auto pieces = palette.format_placings.find( map_key( "X" ) );
    REQUIRE( pieces != palette.format_placings.end() );
    REQUIRE( pieces->second.size() == 4 );
    REQUIRE( palette.keys_with_terrain.count( map_key( "X" ) ) == 1 );

    std::vector<std::string> piece_phases;
    for( const shared_ptr_fast<const jmapgen_piece> &piece : pieces->second ) {
        piece_phases.emplace_back( phase_name( piece->phase() ) );
    }

    JsonObject nested_json = json_loader::from_string( R"({
        "mapgensize": [ 1, 1 ],
        "rows": [ "X" ],
        "palettes": [ "rust_cpp_oracle_mapgen_palette_v1" ]
    })" ).get_object();
    mapgen_function_json_nested nested( std::move( nested_json ),
                                        "Rust static mapgen oracle" );
    nested.setup();
    nested.finalize_parameters();
    const point_rel_ms mapgen_size = nested.get_mapgensize();

    std::ofstream output( output_path, std::ios::out | std::ios::trunc );
    REQUIRE( output.is_open() );
    {
        JsonOut json( output );
        json.start_object();
        json.member( "format_version", 1 );
        json.member( "baseline_commit", baseline_commit );
        json.member( "upstream_tree", upstream_tree );
        json.member( "kernel", "mapgen_static_semantics_v1" );

        json.member( "matching" );
        json.start_array();
        for( const match_case &definition : matching ) {
            json.start_object();
            json.member( "case_id", definition.case_id );
            json.member( "query", definition.query );
            json.member( "terrain_id", definition.terrain_id );
            json.member( "match_type", definition.match_type );
            json.member( "matches", is_ot_match( definition.query,
                         oter_str_id( definition.terrain_id ).id(), definition.type ) );
            json.end_object();
        }
        json.end_array();

        json.member( "rotatable" );
        write_rotations( json, rotatable );
        json.member( "linear" );
        write_rotations( json, linear );

        json.member( "palette" );
        json.start_object();
        json.member( "palette_id", palette_id_text );
        json.member( "key", "X" );
        json.member( "key_has_terrain", true );
        json.member( "piece_phases" );
        json.start_array();
        for( const std::string &phase : piece_phases ) {
            json.write( phase );
        }
        json.end_array();
        json.member( "mapgen_size_x", mapgen_size.x() );
        json.member( "mapgen_size_y", mapgen_size.y() );
        json.member( "setup_completed", true );
        json.end_object();
        json.end_object();
    }
    output.close();
    REQUIRE( output.good() );
}
