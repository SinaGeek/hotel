# Reference-site backend paths (app.hmsflow.com, captured via REA CDP scenarios 2026-10-28)

## Consolidated endpoint inventory

- `/apiv2/application`
- `/apiv2/application/1`
- `/apiv2/application/10`
- `/apiv2/application/11`
- `/apiv2/application/12`
- `/apiv2/application/2`
- `/apiv2/application/3`
- `/apiv2/application/4`
- `/apiv2/application/5`
- `/apiv2/application/6`
- `/apiv2/application/7`
- `/apiv2/application/8`
- `/apiv2/application/9`
- `/apiv2/applicationcategory`
- `/apiv2/booking&filterBookingType%5B%5D=1&filterBookingType%5B%5D=2&status=1&checkoutIsNullOrZero=1`
- `/apiv2/category?bolum=1`
- `/apiv2/category?bolum=3`
- `/apiv2/channel`
- `/apiv2/channel/booking&filterArchive=0`
- `/apiv2/country`
- `/apiv2/currency-exchange`
- `/apiv2/current&isCari=0`
- `/apiv2/current&isCari=0&donemsel=0`
- `/apiv2/current&isCari=1`
- `/apiv2/declaration`
- `/apiv2/expense`
- `/apiv2/guest`
- `/apiv2/home`
- `/apiv2/home/ckeckin/3607/list`
- `/apiv2/home/ckeckout/3607/list`
- `/apiv2/home/warning`
- `/apiv2/invoice&bolum=1`
- `/apiv2/invoice&bolum=2`
- `/apiv2/invoice/lastinvoice`
- `/apiv2/monthly-room/booking`
- `/apiv2/monthly-room/booking/unassigned/count`
- `/apiv2/monthly-room/room`
- `/apiv2/packethotel`
- `/apiv2/paymenttypehotel&status=1`
- `/apiv2/periodic/advanced/channels`
- `/apiv2/periodic/advanced/rooms?channels%5B%5D=24043`
- `/apiv2/periodic/advanced/update/room&channels%5B%5D=24043&rooms%5B%5D=19794&days%5B%5D=1&days%5B%5D=2&days%5B%5D=3&days%5B%5D=4&days%5B%5D=5&days%5B%5D=6&days%5B%5D=7`
- `/apiv2/periodic/advanced/update/room&channels%5B%5D=24043&rooms%5B%5D=19794&rooms%5B%5D=19795&rooms%5B%5D=19796&rooms%5B%5D=19797&rooms%5B%5D=19798&rooms%5B%5D=19799&rooms%5B%5D=19800&days%5B%5D=1&days%5B%5D=2&days%5B%5D=3&days%5B%5D=4&days%5B%5D=5&days%5B%5D=6&days%5B%5D=7`
- `/apiv2/periodic/bring/accommodation-price`
- `/apiv2/periodic/bring/availability`
- `/apiv2/periodic/bring/date`
- `/apiv2/periodic/bring/restriction`
- `/apiv2/periodic/bring/room`
- `/apiv2/rate/type/active/list`
- `/apiv2/report/forecast&hotels%5B%5D=4442`
- `/apiv2/report/guest/nationality&startHour=00%3A00&endHour=23%3A59&hotels%5B%5D=4442`
- `/apiv2/report/inhouse/list&hotels%5B%5D=4442`
- `/apiv2/report/maincourante&hotels%5B%5D=4442`
- `/apiv2/report/meal-list/list&hotels%5B%5D=4442`
- `/apiv2/room`
- `/apiv2/room/housekeeping`
- `/apiv2/roomtypehotel&status=1`
- `/apiv2/sale/public/bill`
- `/apiv2/staff&rule=ROLE_MODERATOR&filterStaffActive=1`
- `/apiv2/stock`
- `/apiv2/stock/category&filterStatus=1`
- `/apiv2/user/permission/section&filterStockStatus=1`

## Endpoints seen per page

### action-expense

- `/apiv2/expense?limit=20&page=1&order=-id`
- `/apiv2/paymenttypehotel?limit=9999&page=1&status=1`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### action-public-sale

- `/apiv2/sale/public/bill?limit=999&page=1&order=-id`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### action-stock

- `/apiv2/stock?limit=20&page=1&order=-id`
- `/apiv2/stock/category?limit=9999&page=1&order=-id&filterStatus=1`
- `/apiv2/user/permission/section?limit=9999&page=1&order=-id&filterStockStatus=1`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### agencies

- `/apiv2/current?limit=20&page=1&order=-id&isCari=0`
- `/apiv2/declaration?order=-id&limit=50&page=1`
- `/apiv2/category?bolum=3`

### bookings

- `/apiv2/booking?limit=20&page=1&order=-id&filterBookingType%5B%5D=1&filterBookingType%5B%5D=2&status=1&checkoutIsNullOrZero=1`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### channel-bookings

- `/apiv2/channel/booking?limit=20&page=1&order=-id&filterArchive=0`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### currents

- `/apiv2/current?limit=20&page=1&order=-id&isCari=1`
- `/apiv2/declaration?order=-id&limit=50&page=1`
- `/apiv2/category?bolum=3`

### dash

- `/apiv2/home`
- `/apiv2/home/warning?date=2026-10-28`
- `/apiv2/home/ckeckin/3607/list?limit=100&page=1&startDate=2026-10-28+19%3A10%3A15`
- `/apiv2/home/ckeckout/3607/list?limit=100&page=1&endDate=2026-10-28`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### facility-accommodation-types

- `/apiv2/rate/type/active/list`
- `/apiv2/channel?limit=9999&page=1`
- `/apiv2/declaration?order=-id&limit=50&page=1`
- `/apiv2/periodic/bring/room?channelID=24043`

### facility-facilities

- `/apiv2/declaration?order=-id&limit=50&page=1`

### facility-housekeeping

- `/apiv2/room/housekeeping`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### facility-room-types

- `/apiv2/roomtypehotel?limit=9999&page=1&status=1`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### facility-rooms

- `/apiv2/declaration?order=-id&limit=50&page=1`

### facility-update-status

- `/apiv2/declaration?order=-id&limit=50&page=1`
- `/apiv2/room?limit=9999&page=1`

### guests

- `/apiv2/guest?limit=20&page=1&order=-id`
- `/apiv2/category?bolum=1`
- `/apiv2/country`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### help

- `/apiv2/declaration?order=-id&limit=50&page=1`

### hotel-info-packages

- `/apiv2/packethotel?limit=20&page=1&order=-id`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### invoices-purchase

- `/apiv2/invoice?limit=20&page=1&order=-id&bolum=2`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### invoices-sales

- `/apiv2/invoice?limit=20&page=1&order=-id&bolum=1`
- `/apiv2/declaration?order=-id&limit=50&page=1`
- `/apiv2/invoice/lastinvoice`

### monthly

- `/apiv2/monthly-room/booking?start=2026-10-28&end=2026-11-11`
- `/apiv2/monthly-room/room?start=2026-10-28&end=2026-11-11`
- `/apiv2/monthly-room/booking/unassigned/count?start=2026-10-28&end=2026-11-11`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### notifications

- `/apiv2/declaration?order=-id&limit=50&page=1`

### periodic-availability

- `/apiv2/declaration?order=-id&limit=50&page=1`
- `/apiv2/periodic/advanced/channels`
- `/apiv2/periodic/advanced/rooms?channels%5B%5D=24043`
- `/apiv2/periodic/advanced/update/room?start=28.10.2026&end=10.11.2026&channels%5B%5D=24043&rooms%5B%5D=19794&days%5B%5D=1&days%5B%5D=2&days%5B%5D=3&days%5B%5D=4&days%5B%5D=5&days%5B%5D=6&days%5B%5D=7`

### periodic-bulk

- `/apiv2/declaration?order=-id&limit=50&page=1`
- `/apiv2/periodic/advanced/channels`
- `/apiv2/current?limit=999&page=1&isCari=0&donemsel=0`

### periodic-calendar

- `/apiv2/periodic/bring/date?startDate=2026-10-28&count=14`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19794&channelID=24043`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19795&channelID=24043`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19796&channelID=24043`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19797&channelID=24043`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19798&channelID=24043`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19799&channelID=24043`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19800&channelID=24043`
- `/apiv2/periodic/bring/accommodation-price?startDate=2026-10-28&count=14&roomTypeHotelID=19794&channelID=24043&accommodationHotelID=6175`
- `/apiv2/periodic/bring/restriction?startDate=2026-10-28&count=14&roomTypeHotelID=19794&channelID=24043&accommodationHotelID=6175`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### periodic-overview

- `/apiv2/declaration?order=-id&limit=50&page=1`
- `/apiv2/periodic/advanced/channels`
- `/apiv2/periodic/advanced/rooms?channels%5B%5D=24043`
- `/apiv2/periodic/advanced/update/room?start=28.10.2026&end=10.11.2026&channels%5B%5D=24043&rooms%5B%5D=19794&rooms%5B%5D=19795&rooms%5B%5D=19796&rooms%5B%5D=19797&rooms%5B%5D=19798&rooms%5B%5D=19799&rooms%5B%5D=19800&days%5B%5D=1&days%5B%5D=2&days%5B%5D=3&days%5B%5D=4&days%5B%5D=5&days%5B%5D=6&days%5B%5D=7`

### periodic

- `/apiv2/periodic/bring/date?startDate=2026-10-28&count=14`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19794&channelID=24043`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19795&channelID=24043`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19796&channelID=24043`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19797&channelID=24043`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19798&channelID=24043`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19799&channelID=24043`
- `/apiv2/periodic/bring/availability?startDate=2026-10-28&count=14&roomTypeHotelID=19800&channelID=24043`
- `/apiv2/periodic/bring/accommodation-price?startDate=2026-10-28&count=14&roomTypeHotelID=19794&channelID=24043&accommodationHotelID=6175`
- `/apiv2/periodic/bring/restriction?startDate=2026-10-28&count=14&roomTypeHotelID=19794&channelID=24043&accommodationHotelID=6175`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### reports-forecast

- `/apiv2/report/forecast?startDate=2026-10-28&hotels%5B%5D=4442`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### reports-inhouse

- `/apiv2/report/inhouse/list?date=2026-10-28&hotels%5B%5D=4442`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### reports-maincourante

- `/apiv2/report/maincourante?date=2026-10-28&hotels%5B%5D=4442`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### reports-meal-list

- `/apiv2/report/meal-list/list?date=2026-10-28&hotels%5B%5D=4442`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### reports-nationality

- `/apiv2/report/guest/nationality?startDate=2026-10-01&endDate=2026-10-28&startHour=00%3A00&endHour=23%3A59&hotels%5B%5D=4442`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### reports

- `/apiv2/declaration?order=-id&limit=50&page=1`

### tools-applications

- `/apiv2/application?limit=100&page=1`
- `/apiv2/declaration?order=-id&limit=50&page=1`
- `/apiv2/applicationcategory?limit=100&page=1`
- `/apiv2/application/1`
- `/apiv2/application/2`
- `/apiv2/application/3`
- `/apiv2/application/4`
- `/apiv2/application/5`
- `/apiv2/application/6`
- `/apiv2/application/7`
- `/apiv2/application/8`
- `/apiv2/application/9`
- `/apiv2/application/10`
- `/apiv2/application/11`
- `/apiv2/application/12`

### tools-channels

- `/apiv2/channel?limit=20&page=1&order=-id`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### tools-currency-exchange

- `/apiv2/currency-exchange?limit=20&page=1&order=-id`
- `/apiv2/declaration?order=-id&limit=50&page=1`

### tools-settings

- `/apiv2/declaration?order=-id&limit=50&page=1`

### tools-staff

- `/apiv2/staff?limit=20&offset=0&order=-id&rule=ROLE_MODERATOR&filterStaffActive=1`
- `/apiv2/declaration?order=-id&limit=50&page=1`
