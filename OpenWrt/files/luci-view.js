'use strict';
'require view';
'require form';
'require fs';
'require poll';
'require ui';

return view.extend({
    render: function() {
        var map = new form.Map('ncut-autologin', 'NCUT 校園網路');
        var section = map.section(form.NamedSection, 'main', 'login');
        section.addremove = false;
        var enabled = section.option(form.Flag, 'enabled', '自動重新連線');
        enabled.rmempty = false;
        var username = section.option(form.Value, 'username', '校園帳號');
        username.rmempty = false;
        var password = section.option(form.Value, 'password', '密碼');
        password.password = true;
        password.rmempty = false;
        var interval = section.option(form.Value, 'interval', '檢查間隔（秒）');
        interval.datatype = 'range(5,3600)';
        interval.default = '15';
        interval.rmempty = false;
        var portal = section.option(form.Value, 'portal_url', '校園入口網址（選填）');
        portal.description = '填寫所在網段的 HTTP 或 HTTPS 閘道網址，包含通訊埠。';
        portal.validate = function(sectionId, value) {
            if (!value) return true;
            try {
                var address = new URL(value);
                if (/^https?:\/\/[^/?#]+(?:\/(?:login)?)?$/.test(value) &&
                    !address.username && !address.password && !/\s|\\/.test(value)) {
                    return true;
                }
            } catch (error) {
                return '請填寫完整的校園閘道網址';
            }
            return '請填寫閘道網址，可保留結尾的 /login';
        };
        var renewal = section.option(form.Value, 'renewal_minutes', '在線續登間隔（分鐘）');
        renewal.datatype = 'range(0,1440)';
        renewal.default = '30';
        renewal.rmempty = false;
        renewal.description = '填寫入口網址並啟用自動重新連線後生效。填 0 可停用續登。';
        var status = E('span', {}, '正在檢查連線');
        var labels = { online: '網路已連線', needs_login: '等待校園登入', unstable: '等待網路連線', missing_credentials: '尚未設定帳號', login_failed: '登入未完成' };
        function check(command) {
            return fs.exec('/usr/bin/ncut-autologin', [command]).then(function(result) {
                status.textContent = labels[(result.stdout || '').trim()] || '等待網路連線';
            }).catch(function() { status.textContent = '無法確認連線'; });
        }
        var connect = E('button', { class: 'btn cbi-button cbi-button-action', click: function(event) {
            event.preventDefault();
            connect.disabled = true;
            status.textContent = '正在登入';
            check('login').finally(function() { connect.disabled = false; });
        } }, '立即連線');
        poll.add(function() { return check('check'); }, 15);
        return map.render().then(function(settings) {
            return E('div', {}, [E('div', { class: 'cbi-section' }, [E('h3', {}, '連線狀態'), status, E('div', { style: 'margin-top: 1em' }, [connect])]), settings]);
        });
    }
});
