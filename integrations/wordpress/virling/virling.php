<?php
/**
 * Plugin Name: Virling Analytics
 * Plugin URI: https://virling.com
 * Description: Adds the Virling tracking script, so you can see which channels bring paying customers.
 * Version: 0.1.0
 * Requires at least: 6.0
 * Requires PHP: 7.4
 * Author: Virling
 * License: GPLv2 or later
 * License URI: https://www.gnu.org/licenses/gpl-2.0.html
 * Text Domain: virling
 */

if (!defined('ABSPATH')) {
    exit;
}

const VIRLING_OPTION = 'virling_settings';

function virling_settings(): array
{
    $host = wp_parse_url(home_url(), PHP_URL_HOST) ?: '';
    return wp_parse_args(get_option(VIRLING_OPTION, []), [
        'server' => '',
        'site' => preg_replace('/^www\./', '', $host),
        'cookieless' => false,
        'respect_gpc' => false,
        'skip_admins' => true,
    ]);
}

add_action('wp_head', function () {
    $s = virling_settings();
    if ($s['server'] === '' || $s['site'] === '') {
        return;
    }
    if ($s['skip_admins'] && current_user_can('manage_options')) {
        return;
    }
    $attrs = sprintf(
        'defer src="%s" data-site="%s"',
        esc_url(untrailingslashit($s['server']) . '/v.js'),
        esc_attr($s['site'])
    );
    if ($s['cookieless']) {
        $attrs .= ' data-cookieless';
    }
    if ($s['respect_gpc']) {
        $attrs .= ' data-respect-gpc';
    }
    echo "<script $attrs></script>\n";
}, 1);

add_action('admin_menu', function () {
    add_options_page('Virling', 'Virling', 'manage_options', 'virling', 'virling_settings_page');
});

add_action('init', function () {
    register_setting('virling', VIRLING_OPTION, [
        'type' => 'array',
        'sanitize_callback' => function ($in) {
            return [
                'server' => esc_url_raw(trim($in['server'] ?? '')),
                'site' => sanitize_text_field(strtolower(trim($in['site'] ?? ''))),
                'cookieless' => !empty($in['cookieless']),
                'respect_gpc' => !empty($in['respect_gpc']),
                'skip_admins' => !empty($in['skip_admins']),
            ];
        },
    ]);
});

add_filter('plugin_action_links_' . plugin_basename(__FILE__), function ($links) {
    array_unshift($links, '<a href="' . esc_url(admin_url('options-general.php?page=virling')) . '">' . esc_html__('Settings', 'virling') . '</a>');
    return $links;
});

function virling_settings_page()
{
    $s = virling_settings();
    $name = VIRLING_OPTION;
    ?>
    <div class="wrap">
        <h1>Virling</h1>
        <p>Virling shows which channels bring paying customers. Enter the address of your Virling server and the site domain you added there.</p>
        <form method="post" action="options.php">
            <?php settings_fields('virling'); ?>
            <table class="form-table" role="presentation">
                <tr>
                    <th scope="row"><label for="virling-server">Virling server</label></th>
                    <td><input id="virling-server" name="<?php echo esc_attr($name); ?>[server]" type="url" class="regular-text" placeholder="https://stats.example.com" value="<?php echo esc_attr($s['server']); ?>">
                    <p class="description">The script loads from this address, for example https://stats.example.com/v.js.</p></td>
                </tr>
                <tr>
                    <th scope="row"><label for="virling-site">Site domain</label></th>
                    <td><input id="virling-site" name="<?php echo esc_attr($name); ?>[site]" type="text" class="regular-text" value="<?php echo esc_attr($s['site']); ?>">
                    <p class="description">Must match the domain added in Virling.</p></td>
                </tr>
                <tr>
                    <th scope="row">Options</th>
                    <td>
                        <label><input name="<?php echo esc_attr($name); ?>[skip_admins]" type="checkbox" value="1" <?php checked($s['skip_admins']); ?>> Do not track logged-in administrators</label><br>
                        <label><input name="<?php echo esc_attr($name); ?>[cookieless]" type="checkbox" value="1" <?php checked($s['cookieless']); ?>> Cookieless mode</label><br>
                        <label><input name="<?php echo esc_attr($name); ?>[respect_gpc]" type="checkbox" value="1" <?php checked($s['respect_gpc']); ?>> Respect Global Privacy Control</label>
                    </td>
                </tr>
            </table>
            <?php submit_button(); ?>
        </form>
    </div>
    <?php
}
