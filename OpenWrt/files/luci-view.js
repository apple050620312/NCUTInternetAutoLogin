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
