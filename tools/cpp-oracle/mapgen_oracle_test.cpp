#include <algorithm>
#include <cstdlib>
#include <fstream>
#include <string>
#include <utility>
#include <vector>

#include "cata_catch.h"
#include "json.h"
#include "json_loader.h"
#include "map.h"
#include "mapgen.h"
#include "mapgendata.h"
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
                            terrain->get_rotation(), marker.rotate( terrain->get_rotation(), dimensions ) } );
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
                            terrain->get_rotation(), marker.rotate( terrain->get_rotation(), dimensions ) } );
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

    constexpr const char *static_palette_id_text = "rust_cpp_oracle_admitted_static_v1";
    JsonObject static_palette_json = json_loader::from_string( R"({
        "id": "rust_cpp_oracle_admitted_static_v1",
        "terrain": { ".": "t_dirt", "X": "t_floor" },
        "furniture": { "X": "f_table" }
    })" ).get_object();
    mapgen_palette::load( static_palette_json, "dda" );
    const mapgen_palette &static_palette = mapgen_palette::get( palette_id( static_palette_id_text ) );
    const auto static_pieces = static_palette.format_placings.find( map_key( "X" ) );
    REQUIRE( static_pieces != static_palette.format_placings.end() );
    REQUIRE( static_pieces->second.size() == 2 );
    std::vector<std::string> static_piece_phases;
    for( const shared_ptr_fast<const jmapgen_piece> &piece : static_pieces->second ) {
        static_piece_phases.emplace_back( phase_name( piece->phase() ) );
    }
    std::string static_rows = "{\"mapgensize\":[24,24],\"rows\":[";
    for( int y = 0; y < 24; ++y ) {
        if( y != 0 ) {
            static_rows += ',';
        }
        std::string row( 24, '.' );
        if( y == 5 ) {
            row[2] = 'X';
        }
        static_rows += '\"' + row + '\"';
    }
    static_rows += "],\"palettes\":[\"rust_cpp_oracle_admitted_static_v1\"]}";
    JsonObject static_nested_json = json_loader::from_string( static_rows ).get_object();
    mapgen_function_json_nested static_nested( std::move( static_nested_json ),
            "Rust admitted static mapgen oracle" );
    static_nested.setup();
    static_nested.finalize_parameters();
    const point_rel_ms static_size = static_nested.get_mapgensize();
    REQUIRE( static_size.x() == 24 );
    REQUIRE( static_size.y() == 24 );
    tinymap generated;
    generated.load( tripoint_abs_omt::zero, true );
    map *generated_map = generated.cast_to_map();
    for( const tripoint_bub_ms &p : generated_map->points_on_zlevel( 0 ) ) {
        generated_map->i_clear( p );
        generated_map->furn_set( p, furn_id( "f_null" ) );
        generated_map->trap_set( p, trap_id( "tr_null" ) );
        generated_map->ter_set( p, ter_id( "t_dirt" ) );
    }
    mapgendata generated_data( *generated_map, mapgendata::dummy_settings );
    static_nested.nest( generated_data, tripoint_rel_ms::zero, "Rust admitted static oracle" );
    const tripoint_bub_ms background( 0, 0, 0 );
    const tripoint_bub_ms static_marker( 2, 5, 0 );
    std::vector<std::string> generated_rows;
    int generated_markers = 0;
    for( int y = 0; y < 24; ++y ) {
        std::string row;
        row.reserve( 24 );
        for( int x = 0; x < 24; ++x ) {
            const tripoint_bub_ms p( x, y, 0 );
            const std::string terrain_id = generated_map->ter( p ).id().str();
            const std::string furniture_id = generated_map->furn( p ).id().str();
            if( terrain_id == "t_floor" && furniture_id == "f_table" ) {
                row += 'X';
                ++generated_markers;
            } else {
                INFO( "unexpected generated tile at " << p.to_string() << ": " <<
                      terrain_id << '/' << furniture_id );
                REQUIRE( terrain_id == "t_dirt" );
                REQUIRE( furniture_id == "f_null" );
                row += '.';
            }
        }
        generated_rows.emplace_back( std::move( row ) );
    }
    REQUIRE( generated_markers == 1 );

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

        json.member( "static_template" );
        json.start_object();
        json.member( "width_tiles", static_size.x() );
        json.member( "height_tiles", static_size.y() );
        json.member( "source_marker_x", static_marker.x() );
        json.member( "source_marker_y", static_marker.y() );
        json.member( "background_terrain_id", "t_dirt" );
        json.member( "marker_terrain_id", "t_floor" );
        json.member( "marker_furniture_id", "f_table" );
        json.member( "generated_background_terrain_id", generated_map->ter( background ).id().str() );
        json.member( "generated_marker_terrain_id", generated_map->ter( static_marker ).id().str() );
        json.member( "generated_marker_furniture_id", generated_map->furn( static_marker ).id().str() );
        json.member( "generated_rows", generated_rows );
        json.member( "piece_phases" );
        json.start_array();
        for( const std::string &phase : static_piece_phases ) {
            json.write( phase );
        }
        json.end_array();
        json.member( "setup_completed", true );
        json.end_object();
        json.end_object();
    }
    output.close();
    REQUIRE( output.good() );
}
