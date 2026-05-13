from django.shortcuts import render, redirect
from django.contrib.auth import get_user_model, authenticate, login, logout
from django.core.mail import send_mail
from django.urls import reverse
from django.conf import settings
from django.http import HttpResponse, JsonResponse
from django.contrib.auth.decorators import login_required
from django.db.models import Q
from django.utils.translation import gettext as _
from django.core.exceptions import PermissionDenied

User = get_user_model()

@login_required
def logout_view(request):
    logout(request)
    return redirect('dashboard')

@login_required
def profile_view(request):
    from .models import ActivityLog
    if request.method == 'POST':
        request.user.first_name = request.POST.get('first_name')
        request.user.last_name = request.POST.get('last_name')
        password = request.POST.get('password')
        if password:
            request.user.set_password(password)
        if 'avatar' in request.FILES:
            request.user.avatar = request.FILES['avatar']
        request.user.save()
        
        ActivityLog.objects.create(
            user=request.user, 
            action=_("Profile Updated"), 
            is_visible_to_user=True
        )
        
        return render(request, 'accounts/profile.html', {
            'message': _('Information saved successfully.'),
            'logs': ActivityLog.objects.filter(user=request.user, is_visible_to_user=True)
        })
    
    context = {
        'logs': ActivityLog.objects.filter(user=request.user, is_visible_to_user=True)
    }
    return render(request, 'accounts/profile.html', context)

def login_view(request):
    # 1. If already logged in, get them out of the login page
    if request.user.is_authenticated:
        return redirect('dashboard')

    # 2. Handle AJAX email checks
    if request.GET.get('check_email'):
        email = request.GET.get('check_email')
        exists = User.objects.filter(Q(email=email) | Q(username=email)).exists()
        return JsonResponse({'exists': exists})

    # 3. Handle Login Attempt
    if request.method == 'POST':
        email = request.POST.get('email')
        password = request.POST.get('password')
        
        if not password:
            return render(request, 'accounts/login.html', {'error': _('Password required'), 'initial_email': email})

        user_obj = User.objects.filter(Q(email=email) | Q(username=email)).first()
        
        if user_obj:
            user = authenticate(request, username=user_obj.username, password=password)
            if user is not None:
                login(request, user)
                return redirect('dashboard')
            else:
                return render(request, 'accounts/login.html', {'error': _('Invalid password'), 'initial_email': email})
        else:
            # User doesn't exist, send to register
            return redirect(f"{reverse('register')}?email={email}")

    # 4. Fallback: Always render the login page for GET requests
    return render(request, 'accounts/login.html')

def register_view(request):
    if request.user.is_authenticated:
        return redirect('dashboard')

    if request.method == 'POST':
        email = request.POST.get('email')
        password = request.POST.get('password')
        first_name = request.POST.get('first_name')
        last_name = request.POST.get('last_name')
        
        if User.objects.filter(email=email).exists():
            return render(request, 'accounts/login.html', {
                'error': _('Email already exists.'), 
                'initial_email': email
            })

        new_user = User.objects.create_user(
            username=email, email=email, password=password,
            first_name=first_name, last_name=last_name, is_active=False
        )
        
        activation_link = request.build_absolute_uri(reverse('activate_user', args=[new_user.id]))
        rejection_link = request.build_absolute_uri(reverse('reject_user', args=[new_user.id]))
        
        send_mail(
            _('New Membership Request'),
            _('User {email} registered.\nApprove: {activation_link}\nReject: {rejection_link}').format(
                email=email, activation_link=activation_link, rejection_link=rejection_link
            ),
            settings.DEFAULT_FROM_EMAIL,
            [admin_email for admin_name, admin_email in settings.ADMINS],
            fail_silently=False,
        )
        return render(request, 'accounts/registration_success.html')

    initial_email = request.GET.get('email', '')
    return render(request, 'accounts/register.html', {'initial_email': initial_email})

@login_required
def activate_user_view(request, user_id):
    if not request.user.is_superuser:
        return HttpResponse(_("Forbidden"), status=403)
        
    from .models import Role, ActivityLog
    try:
        user = User.objects.get(id=user_id)
    except User.DoesNotExist:
        return HttpResponse(_("User not found"), status=404)

    if request.method == 'POST':
        role_id = request.POST.get('role_id')
        try:
            role = Role.objects.get(id=role_id)
            user.role = role
            user.is_active = True
            user.save()
            ActivityLog.objects.create(user=user, action=_("Activated as {role_name}").format(role_name=role.name), is_visible_to_user=True)
            return HttpResponse(f'<script>alert("{_("User {email} Activated").format(email=user.email)}"); window.location.href="/";</script>')
        except Exception as e:
            return HttpResponse(f"{_('Error')}: {e}", status=400)
            
    roles = Role.objects.all()
    role_options = "".join([f'<option value="{r.id}">{r.name}</option>' for r in roles])
    html = f'''
    <html><body>
    <h3>{_('Approve')}: {user.email}</h3>
    <form method="post">
        <input type="hidden" name="csrfmiddlewaretoken" value="{request.COOKIES.get('csrftoken', '')}">
        <select name="role_id" required>{role_options}</select>
        <button type="submit">{_('Activate')}</button>
    </form>
    </body></html>
    '''
    return HttpResponse(html)

@login_required
def reject_user_view(request, user_id):
    if not request.user.is_superuser:
        return HttpResponse(_("Forbidden"), status=403)
        
    try:
        user = User.objects.get(id=user_id)
        if not user.is_active:
            email = user.email
            user.delete()
            return HttpResponse(f'<script>alert("{_("User {email} rejected and deleted.").format(email=email)}"); window.location.href="/";</script>')
        return HttpResponse(_("User already active"), status=400)
    except User.DoesNotExist:
        return HttpResponse(_("Not found"), status=404)